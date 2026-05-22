//! Single-unit compilation: compile one codegen unit into a `.ll` or `.o` file.
//!
//! Extracted from `per_module/mod.rs` to reduce structural complexity.
//! These functions handle the inner loop of unit-level compilation:
//! initializing codegen, declaring defs, compiling bodies, and writing output.

use std::collections::HashSet;
use std::path::Path;

use tungsten_bootstrap::driver::ModuleCodegenUnit;
use tungsten_codegen::inkwell::context::Context as LlvmContext;
use tungsten_codegen::CodeGen;

use super::compilation::{compile_unit_defs, declare_unit_defs};
use super::{codegen_unit_name, UnitCompileCtx};

/// Output location and format for a single codegen unit.
pub(super) struct OutputConfig<'a> {
    pub(super) path: &'a Path,
    pub(super) emit_obj: bool,
}

/// Compile a single codegen unit into a `.ll` file.
///
/// # Ordering invariant (ADR 8.5.26g §2.4)
///
/// Mono instances are pre-seeded into `MonomorphState` via `register_mono_instance`
/// **before** `compile_unit_defs`. This ensures `compile_ty_app` finds pre-assigned
/// symbols instead of generating fresh per-unit names. The sequence is:
///
/// 1. `declare_unit_defs` — declares own + cross-module defs, including non-owner mono declares
/// 2. `register_term_defs` — registers all term bodies for monomorphization lookups
/// 3. **`register_mono_instance`** — pre-seeds MonomorphState with owner-assigned symbols
/// 4. `compile_unit_defs` — compiles function bodies (call sites resolve via pre-seeded map)
/// 5. Owner mono defines — emits `define` for owned instances
pub(super) fn compile_single_unit<'ctx>(
    llvm_context: &'ctx LlvmContext,
    unit: &ModuleCodegenUnit,
    ctx: &UnitCompileCtx<'_>,
    output: &OutputConfig<'_>,
    unit_referenced_globals: &HashSet<String>,
) -> Result<(), String> {
    let unit_name = codegen_unit_name(
        &unit.source_file,
        ctx.project.source_root,
        &unit.defs[0].name,
    );
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("compile_unit", unit = %unit_name).entered();
    let mut codegen = init_codegen(llvm_context, &unit_name, ctx);

    let extern_name_map =
        declare_unit_defs(&mut codegen, unit, &unit_name, ctx, unit_referenced_globals)?;

    // WHY: compile_ty_app → extract_poly_body needs term bodies for on-demand
    // monomorphization. Only Forall-typed defs actually need registration, but
    // currently all are cloned. See ADR 9.5.26e §2.2 for the targeted filter.
    //
    // ADR 10.5.26h §2.1: Bulk-register shared poly term registry instead of
    // iterating all units per worker. One HashMap::clone() per worker.
    {
        #[cfg(feature = "profile")]
        let _span = tracing::info_span!("register_bulk").entered();
        codegen.register_term_defs_bulk(ctx.poly_term_registry);
    }

    // Pre-seed mono instances for call-site resolution (ADR 8.5.26g §2.4).
    // Seeds MonomorphState so compile_ty_app finds pre-assigned symbols
    // instead of generating fresh per-unit names.
    for (key, ownership) in ctx.mono.map.entries() {
        codegen.register_mono_instance(&key.def_id.name, &ownership.type_args, &ownership.symbol);
    }
    // Activate the fallback guard: no ad-hoc mono generation past this point.
    codegen.activate_mono_map();

    compile_unit_defs(&mut codegen, unit, &extern_name_map, &unit_name, ctx)?;

    // Note: mono defines are emitted exclusively in the __mono depot unit
    // (ADR 9.5.26b §2.3). Per-function units never own mono instances.

    // If this module contains "main", create the main wrapper
    if unit.defs.iter().any(|d| d.name == "main") {
        if let Err(e) = codegen.compile_main_wrapper(ctx.project.main_ty) {
            return Err(format!(
                "main wrapper failed in module '{}': {}",
                unit_name, e
            ));
        }
    }

    codegen.finalize_debug_info();
    if output.emit_obj {
        write_obj(&codegen, output.path, &unit_name, ctx.flags.verbose)
    } else {
        write_ll(&codegen, output.path, &unit_name, ctx.flags.verbose)
    }
}

/// Initialize a `CodeGen` instance with shared configuration.
pub(super) fn init_codegen<'ctx>(
    llvm_context: &'ctx LlvmContext,
    unit_name: &str,
    ctx: &UnitCompileCtx<'_>,
) -> CodeGen<'ctx> {
    let mut codegen = CodeGen::new(llvm_context, unit_name);
    // Set module prefix so generated names (lambdas, monomorphized instances)
    // don't collide across codegen units.
    codegen.set_module_prefix(unit_name.to_string());
    if ctx.flags.debug_info {
        let source_text = std::fs::read_to_string(ctx.project.file).unwrap_or_default();
        let source_path = ctx.project.file.to_string_lossy();
        codegen.enable_debug_info(&source_path, &source_text);
    }
    apply_diagnostic_flags(&mut codegen, ctx);
    codegen.register_record_types(ctx.project.record_types.clone());
    codegen.register_adt_types(ctx.project.adt_types.clone());
    codegen
}

/// Apply diagnostic and tracing flags from compilation context to a CodeGen instance.
fn apply_diagnostic_flags(codegen: &mut CodeGen<'_>, ctx: &UnitCompileCtx<'_>) {
    if ctx.flags.diagnostics.codegen_backtrace {
        codegen.set_codegen_backtrace(true);
    }
    if let Some(ref filter) = ctx.flags.diagnostics.tracing.trace_adt_ops {
        codegen.set_trace_adt_ops(filter.clone());
    }
    if ctx.flags.diagnostics.tracing.trace_musttail {
        codegen.set_trace_musttail();
    }
    if ctx.flags.diagnostics.tracing.trace_escape {
        codegen.set_trace_escape();
    }
    if ctx.flags.named_lambdas {
        codegen.set_named_lambdas(true);
    }
    if let Some(ref filter) = ctx.flags.diagnostics.alloc_profile {
        let filter_opt = if filter.is_empty() {
            None
        } else {
            Some(filter.clone())
        };
        codegen.set_alloc_profile(filter_opt);
    }
}

/// Write LLVM IR to a `.ll` file.
pub(super) fn write_ll(
    codegen: &CodeGen<'_>,
    ll_path: &Path,
    unit_name: &str,
    verbose: bool,
) -> Result<(), String> {
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("write_ll", unit = %unit_name).entered();
    let ir = codegen.get_ir_string();
    std::fs::write(ll_path, ir)
        .map_err(|e| format!("could not write '{}': {}", ll_path.display(), e))?;
    if verbose {
        eprintln!("  Wrote {} (module '{}')", ll_path.display(), unit_name);
    }
    Ok(())
}

/// Write a native object file directly from the in-memory LLVM module (ADR 9.5.26e §2.1).
pub(super) fn write_obj(
    codegen: &CodeGen<'_>,
    obj_path: &Path,
    unit_name: &str,
    verbose: bool,
) -> Result<(), String> {
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("write_obj", unit = %unit_name).entered();
    codegen
        .write_object_file_with_opt(obj_path, tungsten_codegen::inkwell::OptimizationLevel::None)
        .map_err(|e| format!("could not write object '{}': {}", obj_path.display(), e))?;
    if verbose {
        eprintln!("  Wrote {} (module '{}')", obj_path.display(), unit_name);
    }
    Ok(())
}
