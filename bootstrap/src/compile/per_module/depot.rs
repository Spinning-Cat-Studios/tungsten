//! `__mono` depot codegen unit (ADR 9.5.26b §2.3).
//!
//! All monomorphized specializations are compiled into this single synthetic
//! codegen unit. Per-function units declare (but never define) specializations;
//! definitions live exclusively here.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use tungsten_codegen::inkwell::context::Context as LlvmContext;
use tungsten_core::terms::Term;

use super::compilation::{collect_globals_from_term, declare_referenced_externs};
use super::unit_compile::{init_codegen, write_ll, write_obj};
use super::{CompiledModule, OutputKind, UnitCompileCtx};
use crate::compile::def_llvm_name;
use crate::compile::mono;

/// Compile the `__mono` depot unit.
pub(super) fn compile_mono_depot_unit<'ctx>(
    llvm_context: &'ctx LlvmContext,
    owned: &[&mono::MonoOwnership],
    ctx: &UnitCompileCtx<'_>,
    output_path: &Path,
    emit_obj: bool,
) -> Result<(), String> {
    let unit_name = mono::MONO_DEPOT_UNIT;
    let mut codegen = init_codegen(llvm_context, unit_name, ctx);

    // ADR 10.5.26h §2.1: Use shared poly term registry instead of per-worker iteration
    codegen.register_term_defs_bulk(ctx.poly_term_registry);
    for (key, info) in ctx.all_defs_info {
        let original = key.split("::").last().unwrap_or(key);
        let original_llvm = def_llvm_name(original);
        if matches!(&info.ty, tungsten_core::types::Type::Forall(_, _)) {
            codegen.register_def_type(&info.llvm_name, &info.ty);
            if info.llvm_name != original_llvm {
                codegen.register_def_type(&original_llvm, &info.ty);
            }
        }
    }

    // ADR 1.7.26f: declare the non-generic top-level functions the owned
    // instances' bodies call, as external prototypes — their definitions live
    // in per-function units and the linker connects them. Global names are
    // specialization-invariant (monomorphization substitutes types, not callee
    // names), so collecting from the generic bodies covers the specialized
    // instances. `owned` is the complete frozen instance set: the single-owner
    // ownership map is computed by discovery before any codegen (ADR 8.5.26g),
    // and `declare_referenced_externs` is idempotent if that ever changes.
    let instance_refs: Vec<BTreeSet<String>> = owned
        .iter()
        .map(|ownership| {
            referenced_globals_of_instance(ctx.poly_term_registry, &ownership.key.def_id.name)
        })
        .collect();
    let all_referenced: BTreeSet<String> = instance_refs.iter().flatten().cloned().collect();
    let mut extern_name_map: HashMap<String, String> = HashMap::new();
    declare_referenced_externs(
        &mut codegen,
        &all_referenced,
        ctx,
        unit_name,
        &mut extern_name_map,
    )?;

    // Compile each owned mono instance
    for (ownership, refs) in owned.iter().zip(&instance_refs) {
        let global_name = &ownership.key.def_id.name;
        if ctx.flags.diagnostics.tracing.trace_mono {
            eprintln!(
                "[mono]   depot define {} ({})",
                ownership.symbol, ownership.key
            );
        }
        // Colliding referenced names resolve per instance (ADR 12.7.26a D5):
        // own-module def first (a generic body's unqualified callee is a
        // same-module reference, matched on `DefInfo.module_path` — the D4
        // fix for `foo/mod.tg` unit-name divergence), then the instance
        // module's import table, else the D1 hard error.
        let mut instance_map = extern_name_map.clone();
        let instance_label = format!("{}[{}]", unit_name, ownership.symbol);
        instance_map.extend(super::imports::collision_overrides_for_unit(
            &ownership.key.def_id.module_path,
            &instance_label,
            refs,
            &ctx.collision_context(),
        )?);
        codegen.register_extern_name_map(instance_map);
        if let Err(e) = codegen.compile_monomorphized_named(
            global_name,
            &ownership.type_args,
            &ownership.symbol,
        ) {
            return Err(format!(
                "mono depot define failed for '{}': {}",
                ownership.symbol, e
            ));
        }
    }

    if emit_obj {
        write_obj(&codegen, output_path, unit_name, ctx.flags.verbose)
    } else {
        write_ll(&codegen, output_path, unit_name, ctx.flags.verbose)
    }
}

/// Collect the `Global` names referenced by a mono instance's generic body,
/// transitively through its generic callees.
///
/// Bodies live in the shared poly term registry, keyed by (scoped) LLVM
/// name with an original-name alias — the same resolution
/// `compile_monomorphized_named` uses via `extract_poly_body_multi`.
///
/// Transitivity matters (ADR 21.7.26e): compiling an owned instance
/// recursively defines the specializations of the *private generic helpers*
/// it calls (they get no owned instances of their own), so a non-generic
/// top-level function referenced only by a nested callee — e.g.
/// `strmap_insert → strmap_rebalance → strmap_node → taller_of` — still
/// needs an external prototype in the depot. Collecting only the root
/// body's direct refs left such helpers undeclared, failing the depot
/// define with "referenced but not declared".
pub(in crate::compile) fn referenced_globals_of_instance(
    poly_term_registry: &HashMap<String, Term>,
    def_name: &str,
) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut worklist: Vec<String> = vec![def_name.to_string()];
    while let Some(name) = worklist.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let body: Option<&Term> = poly_term_registry
            .get(&def_llvm_name(&name))
            .or_else(|| poly_term_registry.get(&name));
        let Some(body) = body else {
            // Not a registered generic body — a plain top-level function or
            // constructor; it needs a prototype but has nothing to recurse
            // into (it was already inserted into `refs` by its referencer).
            continue;
        };
        let mut direct = BTreeSet::new();
        collect_globals_from_term(body, &mut direct);
        for referenced in direct {
            refs.insert(referenced.clone());
            worklist.push(referenced);
        }
    }
    refs
}

/// Compile the __mono depot unit containing all monomorphized instances (ADR 9.5.26b §2.3).
pub(super) fn compile_mono_depot(
    llvm_context: &LlvmContext,
    ctx: &UnitCompileCtx<'_>,
    output_dir: &Path,
    emit_obj: bool,
    compiled: &mut Vec<CompiledModule>,
) -> Result<(), String> {
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("compile_mono_depot").entered();
    let depot_id = mono::CodegenUnitId::mono_depot();
    let depot_owned = ctx.mono.map.owned_by(&depot_id);
    if !depot_owned.is_empty() {
        let depot_name = mono::MONO_DEPOT_UNIT;
        let (ext, kind) = if emit_obj {
            ("o", OutputKind::Obj)
        } else {
            ("ll", OutputKind::Ll)
        };
        let output_path = output_dir.join(format!("{}.{}", depot_name, ext));
        if ctx.flags.verbose || ctx.flags.diagnostics.tracing.trace_mono {
            eprintln!(
                "[mono] compiling __mono depot ({} owned instance(s))",
                depot_owned.len()
            );
        }
        compile_mono_depot_unit(llvm_context, &depot_owned, ctx, &output_path, emit_obj)?;
        compiled.push(CompiledModule {
            output_path,
            name: depot_name.to_string(),
            kind,
        });
    }
    Ok(())
}
