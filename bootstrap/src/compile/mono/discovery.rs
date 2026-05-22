//! Mono request discovery: walk CoreDefs to find `TyApp(Global(name), ty_arg)`.
//!
//! This pre-pass collects all monomorphization requests before codegen starts,
//! so the ownership map can be frozen and shared immutably during per-unit
//! LLVM IR generation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use tungsten_core::terms::{SpannedTerm, Term};

use tungsten_bootstrap::driver::ModuleCodegenUnit;

use super::{CanonicalTypeArgs, CodegenUnitId, DefId, MonoKey, MonoRequest, MonoRequestTable};

/// Walk all codegen units and collect monomorphization requests.
///
/// A request is recorded whenever a `TyApp(Global(name), ty_arg)` is found
/// in a definition's term body. The `name` is resolved to a `DefId` using
/// the def-to-unit mapping.
///
/// # Limitation: no type-substituted fixed-point discovery
///
/// The current implementation performs a single static walk over all term
/// bodies. It discovers `TyApp(Global(g), ConcreteType)` calls that appear
/// textually in the AST, including inside `TyAbs` bodies.
///
/// However, it does NOT perform type-substituted discovery. If `f<T> = g<T>`
/// and we discover `f<Nat>`, we do not yet substitute `T=Nat` into `f`'s body
/// to discover `g<Nat>`. The static walk finds `g<TyVar("T")>` inside the
/// polymorphic body, but these non-concrete requests are filtered out (they
/// would cause `lower_type` recursion failures in codegen).
///
/// In practice this is acceptable because:
/// 1. Most polymorphic functions in Tungsten call other polymorphic functions
///    with explicit concrete types (e.g., `g<Nat>` inside `f<T>`), which the
///    static walk catches.
/// 2. Type-parameter pass-through (`f<T> = g<T>`) is handled by the fallback
///    guard in `MonomorphState`: if `g<Nat>` is not pre-seeded, codegen
///    reports an ICE with `--trace-mono` diagnostic guidance.
///
/// A future enhancement could add iterative discovery with type substitution
/// to close this gap.
pub fn discover_mono_requests(
    units: &[ModuleCodegenUnit],
    source_root: &Path,
    concrete_type_names: &HashSet<String>,
) -> MonoRequestTable {
    let def_to_unit = build_def_to_unit_map(units);
    let mut table = MonoRequestTable::new();

    for unit in units {
        let unit_id = unit_id_from_module(unit, source_root);
        for def in &unit.defs {
            collect_from_spanned_term(
                &def.term,
                &unit_id,
                &def_to_unit,
                concrete_type_names,
                &mut table,
            );
        }
    }

    table
}

/// Build a map from definition name → DefId for resolving Global references.
///
/// When multiple units define the same name (collisions), the first one wins.
/// This matches the existing codegen collision-resolution strategy.
fn build_def_to_unit_map(units: &[ModuleCodegenUnit]) -> HashMap<String, DefId> {
    let mut map = HashMap::new();
    for unit in units {
        for def in &unit.defs {
            map.entry(def.name.clone())
                .or_insert_with(|| DefId::new(unit.module_path.clone(), &def.name));
        }
    }
    map
}

/// Derive a `CodegenUnitId` from a `ModuleCodegenUnit`.
fn unit_id_from_module(unit: &ModuleCodegenUnit, source_root: &Path) -> CodegenUnitId {
    CodegenUnitId(super::super::per_module::codegen_unit_name(
        &unit.source_file,
        source_root,
        &unit.defs[0].name,
    ))
}

/// Walk a spanned term tree, collecting mono requests.
fn collect_from_spanned_term(
    st: &SpannedTerm,
    requester: &CodegenUnitId,
    def_map: &HashMap<String, DefId>,
    concrete_type_names: &HashSet<String>,
    table: &mut MonoRequestTable,
) {
    collect_from_term(&st.term, requester, def_map, concrete_type_names, table);
}

/// Walk a term tree, recording `TyApp(Global(name), ty_arg)` as mono requests.
///
/// Handles nested `TyApp` for multi-type-parameter generics:
/// `TyApp(TyApp(Global("pmap"), A), B)` → `pmap` with type_args `[A, B]`.
fn collect_from_term(
    term: &Term,
    requester: &CodegenUnitId,
    def_map: &HashMap<String, DefId>,
    concrete_type_names: &HashSet<String>,
    table: &mut MonoRequestTable,
) {
    // The key pattern: peel nested TyApp to find Global at the base
    if let Term::TyApp(_, _) = term {
        // Peel all nested TyApp layers to find (base, [ty_arg_1, ty_arg_2, ...])
        let mut type_args = Vec::new();
        let mut current = term;
        while let Term::TyApp(inner, ty_arg) = current {
            type_args.push(ty_arg.clone());
            current = inner.as_ref();
        }
        type_args.reverse(); // collected outer→inner, need inner→outer order

        if let Term::Global(name) = current {
            // Skip non-concrete type args
            if !type_args
                .iter()
                .any(|a| a.has_mono_blocking_tyvar(concrete_type_names))
            {
                if let Some(def_id) = def_map.get(name) {
                    let normalized: Vec<_> =
                        type_args.iter().map(|a| strip_at_prefixes(a)).collect();
                    let key =
                        MonoKey::new(def_id.clone(), CanonicalTypeArgs::from_types(&normalized));
                    table.add(MonoRequest {
                        key,
                        requester_unit: requester.clone(),
                        type_args: normalized,
                    });
                }
            }
            // We consumed the full TyApp chain — don't recurse into inner TyApps
            // (they'd produce spurious single-arg requests for the same Global).
            return;
        }
        // Base was not Global — fall through to normal recursion
    }

    // Recurse into all sub-terms
    term.for_each_subterm(|child| {
        collect_from_term(child, requester, def_map, concrete_type_names, table);
    });
}

/// Strip `@` prefixes from all TyVars in a type tree.
///
/// `@`-prefixed TyVars are Phase 1c artifacts that reference concrete named types.
/// Stripping them normalizes the type so that `@Token` and `Token` produce
/// identical canonical type args for mono key matching.
///
/// Note: as of ADR 10.5.26d P7, `@`-prefixed TyVars are stripped at the
/// elaboration→codegen boundary (`CoreDef::strip_at_prefixes`). This function
/// is retained as a defense-in-depth safety net — it should be a no-op on
/// well-formed input.
pub(crate) fn strip_at_prefixes(ty: &tungsten_core::types::Type) -> tungsten_core::types::Type {
    ty.strip_tyvar_at_prefix()
}
