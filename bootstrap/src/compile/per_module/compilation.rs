//! Per-unit compilation helpers: declare, register, and compile definitions.
//!
//! Extracted from `per_module/mod.rs` to reduce structural complexity.
//! These functions handle the inner loop of single-unit compilation:
//! declaring own + cross-module defs, registering term bodies, emitting
//! mono references, and compiling function bodies.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use tungsten_bootstrap::driver::ModuleCodegenUnit;
use tungsten_codegen::CodeGen;
use tungsten_core::terms::Term;

use super::{codegen_unit_name, UnitCompileCtx};
use crate::compile::mono;
use crate::compile::{def_llvm_name, extern_wrap_name};

/// Information about a definition for cross-module declaration generation.
pub(in crate::compile) struct DefInfo {
    /// The LLVM name (module-scoped to avoid collisions)
    pub(in crate::compile) llvm_name: String,
    /// The definition's type
    pub(in crate::compile) ty: tungsten_core::types::Type,
    /// Which codegen unit owns this definition
    pub(in crate::compile) owner_unit: String,
}

/// Build a module-scoped LLVM name for a definition.
///
/// Only names in the `collisions` set get prefixed with `<unit_name>__`.
/// All other names pass through unchanged. This avoids breaking the many
/// codegen lookup paths that assume original names (monomorphization,
/// direct calls, type inference, etc.), while still preventing linker
/// collisions for same-named functions in different modules.
pub(super) fn scoped_llvm_name(
    def_name: &str,
    unit_name: &str,
    collisions: &HashSet<String>,
) -> String {
    let base = def_llvm_name(def_name);
    if collisions.contains(&base) {
        format!("{}__{}", unit_name, base)
    } else {
        base
    }
}

/// Detect which definition names appear in multiple codegen units.
///
/// Returns a set of names that need module-scoping to avoid linker collisions.
/// Names that are unique across all units don't need scoping.
pub(super) fn find_colliding_names(units: &[ModuleCodegenUnit]) -> HashSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for unit in units {
        for def in &unit.defs {
            let name = def_llvm_name(&def.name);
            *seen.entry(name).or_insert(0) += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name)
        .collect()
}

/// Build cross-module info: for each definition, its LLVM name and owner.
///
/// Uses a composite key `"unit::def_name"` so that same-named definitions
/// in different modules don't overwrite each other.
pub(super) fn build_cross_module_info(
    units: &[ModuleCodegenUnit],
    collisions: &HashSet<String>,
    source_root: &Path,
) -> BTreeMap<String, DefInfo> {
    let mut info = BTreeMap::new();
    for unit in units {
        let unit_name = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
        for def in &unit.defs {
            let llvm_name = if let Some((_, wrap)) = extern_wrap_name(&def.name, &def.term) {
                wrap
            } else {
                scoped_llvm_name(&def.name, &unit_name, collisions)
            };
            let key = format!("{}::{}", unit_name, def.name);
            info.insert(
                key,
                DefInfo {
                    llvm_name,
                    ty: def.ty.clone(),
                    owner_unit: unit_name.clone(),
                },
            );
        }
    }
    info
}

/// Declare owned + cross-module definitions, returning the extern name map.
///
/// The extern name map maps original names to their LLVM symbol names.
/// For extern wrappers (`__c_foo`), this maps `foo → __wrap_foo`.
/// For regular defs, the names are unchanged (scoping is deferred to ADR 6.5.26d).
pub(super) fn declare_unit_defs(
    codegen: &mut CodeGen<'_>,
    unit: &ModuleCodegenUnit,
    unit_name: &str,
    ctx: &UnitCompileCtx<'_>,
    referenced: &HashSet<String>,
) -> Result<HashMap<String, String>, String> {
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("declare_unit_defs", unit = %unit_name).entered();
    let mut extern_name_map: HashMap<String, String> = HashMap::new();

    // Declare this module's own definitions
    for def in &unit.defs {
        let original = def_llvm_name(&def.name);

        let llvm_name = if let Some((orig, wrap)) = extern_wrap_name(&def.name, &def.term) {
            extern_name_map.insert(orig, wrap.clone());
            wrap
        } else {
            scoped_llvm_name(&def.name, unit_name, ctx.collisions)
        };

        // Map original → scoped for colliding names
        if llvm_name != original {
            extern_name_map.insert(original.clone(), llvm_name.clone());
        }

        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            codegen.register_def_type(&llvm_name, &def.ty);
            // For colliding names, also register with original name for
            // monomorphization lookups (Core IR uses original names)
            if llvm_name != original {
                codegen.register_def_type(&original, &def.ty);
            }
        } else if let Err(e) = codegen.declare_def(&llvm_name, &def.ty) {
            return Err(format!(
                "declaration failed for '{}' in '{}': {}",
                def.name, unit_name, e
            ));
        }
    }

    // Declare cross-module references (targeted: only defs actually referenced by this unit)
    for (key, info) in ctx.all_defs_info {
        if info.owner_unit == unit_name {
            continue;
        }
        // Skip defs not referenced by this unit's term bodies (ADR 9.5.26d §2.2b)
        let original = key.split("::").last().unwrap_or(key);
        if !referenced.contains(original) {
            continue;
        }
        let original_llvm = def_llvm_name(original);
        if info.llvm_name != original_llvm {
            extern_name_map.insert(original_llvm.clone(), info.llvm_name.clone());
        }

        if matches!(&info.ty, tungsten_core::types::Type::Forall(_, _)) {
            codegen.register_def_type(&info.llvm_name, &info.ty);
            // For colliding names, also register with original for monomorphization
            if info.llvm_name != original_llvm {
                codegen.register_def_type(&original_llvm, &info.ty);
            }
        } else if let Err(e) = codegen.declare_def(&info.llvm_name, &info.ty) {
            return Err(format!(
                "cross-module declare failed for '{}' (from '{}') in '{}': {}",
                key, info.owner_unit, unit_name, e
            ));
        }
    }

    // Non-owner mono declare emission (ADR 8.5.26g §2.3)
    declare_mono_references(codegen, unit_name, ctx)?;

    // Register extern name mappings with codegen
    codegen.register_extern_name_map(extern_name_map.clone());

    Ok(extern_name_map)
}

/// Emit `declare` for mono instances this unit references but does not own (ADR 8.5.26g §2.3).
///
/// For each `MonoKey` requested by `unit_name` where the owner is a different unit,
/// compute the specialized type via substitution and emit a `declare`.
fn declare_mono_references(
    codegen: &mut CodeGen<'_>,
    unit_name: &str,
    ctx: &UnitCompileCtx<'_>,
) -> Result<(), String> {
    let unit_id = mono::CodegenUnitId(unit_name.to_string());
    let requested_keys = ctx.mono.table.keys_requested_by(&unit_id);
    for key in &requested_keys {
        let ownership = match ctx.mono.map.get(key) {
            Some(o) => o,
            None => continue, // shouldn't happen after validate, but be safe
        };
        if ownership.owner_unit.0 == unit_name {
            continue; // this unit owns it — will be defined, not declared
        }
        // Compute the specialized type for the declare signature
        if let Some(original_ty) = codegen.get_def_type(&ownership.key.def_id.name) {
            // Peel all Forall layers and substitute each type arg
            let mut current_ty = original_ty.clone();
            for ty_arg in &ownership.type_args {
                if let tungsten_core::types::Type::Forall(var, inner_ty) = &current_ty {
                    current_ty = inner_ty.substitute(var, ty_arg);
                } else {
                    break;
                }
            }
            if current_ty != original_ty {
                if let Err(e) = codegen.declare_def(&ownership.symbol, &current_ty) {
                    return Err(format!(
                        "mono declare failed for '{}' in '{}': {}",
                        ownership.symbol, unit_name, e
                    ));
                }
                if ctx.flags.diagnostics.tracing.trace_mono {
                    eprintln!(
                        "[mono]   declare {} in '{}' (owner={})",
                        ownership.symbol, unit_name, ownership.owner_unit
                    );
                }
            }
        }
    }
    Ok(())
}

/// Build a shared polymorphic term registry for all units (ADR 10.5.26h §2.1).
///
/// Produces the same map that `register_term_defs` would build per-worker,
/// but built once and shared across workers via `register_term_defs_bulk`.
pub(super) fn build_poly_term_registry(
    units: &[ModuleCodegenUnit],
    collisions: &HashSet<String>,
    source_root: &Path,
) -> HashMap<String, Term> {
    let mut registry = HashMap::new();
    for unit in units {
        let unit_name = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
        for def in &unit.defs {
            if !matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
                continue;
            }
            let llvm_name = scoped_llvm_name(&def.name, &unit_name, collisions);
            registry.insert(llvm_name.clone(), def.term.term.clone());
            let original = def_llvm_name(&def.name);
            if llvm_name != original {
                registry.insert(original, def.term.term.clone());
            }
        }
    }
    registry
}

/// Register polymorphic term definitions for monomorphization (including cross-module).
///
/// Only `Forall`-typed definitions need their term bodies registered — monomorphic
/// functions are resolved via `declare`/`define` and don't need term bodies for
/// `extract_poly_body`. This reduces O(N²) cloning to O(N × poly_count).
/// See ADR 9.5.26e §2.2.
pub(super) fn register_term_defs(
    codegen: &mut CodeGen<'_>,
    units: &[ModuleCodegenUnit],
    collisions: &HashSet<String>,
    source_root: &Path,
) {
    for unit in units {
        let unit_name = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
        for def in &unit.defs {
            if !matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
                continue; // monomorphic — resolved via declare/define, no term body needed
            }
            let llvm_name = scoped_llvm_name(&def.name, &unit_name, collisions);
            codegen.register_term_def(&llvm_name, def.term.term.clone());
            // For colliding names, also register with original for monomorphization
            let original = def_llvm_name(&def.name);
            if llvm_name != original {
                codegen.register_term_def(&original, def.term.term.clone());
            }
        }
    }
}

/// Compile all definitions in a codegen unit.
pub(super) fn compile_unit_defs(
    codegen: &mut CodeGen<'_>,
    unit: &ModuleCodegenUnit,
    extern_name_map: &HashMap<String, String>,
    unit_name: &str,
    ctx: &UnitCompileCtx<'_>,
) -> Result<(), String> {
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("compile_unit_defs", unit = %unit_name).entered();
    let total = unit.defs.len();
    for (i, def) in unit.defs.iter().enumerate() {
        let original = def_llvm_name(&def.name);
        let llvm_name = extern_name_map
            .get(&original)
            .cloned()
            .unwrap_or_else(|| scoped_llvm_name(&def.name, unit_name, ctx.collisions));

        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }

        if ctx.flags.verbose {
            eprintln!("  [{}/{}] {} → {}", i + 1, total, def.name, llvm_name);
        }

        if let Err(e) = codegen.compile_def_with_span(
            &llvm_name,
            &def.term.term,
            &def.ty,
            def.term.span.map(|s| s.start),
        ) {
            return Err(format!(
                "codegen failed for '{}' in '{}': {}",
                def.name, unit_name, e
            ));
        }
    }
    Ok(())
}

/// Collect all `Global(name)` references from a codegen unit's definitions.
///
/// Used to filter cross-module declarations: only declare functions that
/// are actually referenced by this unit's term bodies (ADR 9.5.26d §2.2b).
pub(super) fn collect_referenced_globals(unit: &ModuleCodegenUnit) -> HashSet<String> {
    let mut globals = HashSet::new();
    for def in &unit.defs {
        collect_globals_from_term(&def.term.term, &mut globals);
    }
    globals
}

fn collect_globals_from_term(term: &Term, globals: &mut HashSet<String>) {
    if let Term::Global(name) = term {
        globals.insert(name.clone());
    }
    term.for_each_subterm(|child| collect_globals_from_term(child, globals));
}
