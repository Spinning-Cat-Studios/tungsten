//! `__mono` depot codegen unit (ADR 9.5.26b §2.3).
//!
//! All monomorphized specializations are compiled into this single synthetic
//! codegen unit. Per-function units declare (but never define) specializations;
//! definitions live exclusively here.

use std::path::Path;

use tungsten_codegen::inkwell::context::Context as LlvmContext;

use super::unit_compile::{init_codegen, write_ll, write_obj};
use super::{CompiledModule, OutputKind, UnitCompileCtx};
use crate::compile::def_llvm_name;
use crate::compile::mono::{self, MonoOwnershipMap};

/// Compile the `__mono` depot unit.
pub(super) fn compile_prelude_unit<'ctx>(
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

    // Compile each owned mono instance
    for ownership in owned {
        let global_name = &ownership.key.def_id.name;
        if ctx.flags.diagnostics.tracing.trace_mono {
            eprintln!(
                "[mono]   prelude define {} ({})",
                ownership.symbol, ownership.key
            );
        }
        if let Err(e) = codegen.compile_monomorphized_named(
            global_name,
            &ownership.type_args,
            &ownership.symbol,
        ) {
            return Err(format!(
                "prelude mono define failed for '{}': {}",
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
        compile_prelude_unit(llvm_context, &depot_owned, ctx, &output_path, emit_obj)?;
        compiled.push(CompiledModule {
            output_path,
            name: depot_name.to_string(),
            kind,
        });
    }
    Ok(())
}
