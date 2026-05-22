//! Compile command implementation and codegen configuration.

use std::path::PathBuf;
use std::process::ExitCode;

use super::{compile_all_defs, declare_and_register_defs, warn_sorry_defs};
use super::{convert_adt_types_for_codegen, find_main_type, validate_no_codegen_flags};
use super::{diagnostics, linking, per_module, validation};
use super::{CompileFlags, DiagnosticFlags, TraceFlags};
use tungsten_bootstrap::driver::{self, CoreDef};

/// Compile command: compile to native executable.
pub(crate) fn cmd_compile(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    flags: &CompileFlags,
) -> ExitCode {
    if flags.no_codegen {
        if let Err(code) = validate_no_codegen_flags(flags, output) {
            return code;
        }
    }

    // Use driver's elaborate_project for multi-module support
    let trace_opts = driver::TraceOptions {
        trace_types: flags.diagnostics.trace_types.clone(),
        trace_encoding: flags.diagnostics.tracing.trace_encoding.clone(),
        trace_normalization: flags.diagnostics.tracing.trace_normalization.clone(),
        trace_ctor_registration: flags.diagnostics.tracing.trace_constructor_registration,
        ..Default::default()
    };
    let elab_start = std::time::Instant::now();
    let mut project =
        match driver::elaborate_project(file, flags.verbose, flags.max_errors, Some(&trace_opts)) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::FAILURE;
            }
        };
    let elab_elapsed = elab_start.elapsed();

    if flags.verbose {
        eprintln!("[perf] elab={:.2}s", elab_elapsed.as_secs_f64());
        eprintln!("Elaborated {} definition(s)", project.defs.len());
        if !project.record_types.is_empty() {
            eprintln!("Found {} record type(s)", project.record_types.len());
        }
        if !project.adt_types.is_empty() {
            eprintln!("Found {} ADT type(s)", project.adt_types.len());
        }
    }

    if flags.dump_types {
        for def in &project.defs {
            eprintln!("  {} : {}", def.name, driver::format_type(&def.ty));
        }
    }

    // Post-elaboration validation and substitution
    validation::validate_tyvar_escapes(&project.defs, flags.diagnostics.check_tyvar_escape);
    validation::apply_tyvar_substitutions(
        &mut project.defs,
        &project.type_provenance,
        &project.adt_types,
        flags.verbose,
    );

    // Handle diagnostic dump modes (early return — these are useful with or without --no-codegen)
    if let Some(ref pattern) = flags.diagnostics.dump_ir {
        diagnostics::dump_core_ir(pattern, &project.defs, &project.type_provenance);
        return ExitCode::SUCCESS;
    }
    if let Some(ref adt_name) = flags.diagnostics.dump_encoding {
        diagnostics::dump_adt_encoding(adt_name, &project.adt_types, &project.type_provenance);
        return ExitCode::SUCCESS;
    }

    // --no-codegen: stop after Core IR generation, skip LLVM codegen + linking
    if flags.no_codegen {
        eprintln!(
            "[no-codegen] Core IR generated successfully. {} definition(s).",
            project.defs.len()
        );
        return ExitCode::SUCCESS;
    }

    // Check for sorry
    warn_sorry_defs(&project.defs);

    // Find main function
    let main_ty = match find_main_type(file, &project.defs) {
        Ok(ty) => ty,
        Err(code) => return code,
    };

    // Choose codegen strategy: per-module when multi-module, single-module otherwise
    let has_multiple_units = project.codegen_units.len() > 1;
    let codegen_start = std::time::Instant::now();
    let result = if has_multiple_units {
        per_module::run_codegen_per_module(file, output, flags, &project, &main_ty)
    } else {
        run_codegen(file, output, flags, project, &main_ty)
    };
    if flags.verbose {
        let total = elab_start.elapsed();
        let codegen_link = codegen_start.elapsed();
        let elab_pct = elab_elapsed.as_secs_f64() / total.as_secs_f64() * 100.0;
        let codegen_pct = codegen_link.as_secs_f64() / total.as_secs_f64() * 100.0;
        eprintln!(
            "[perf] total={:.2}s elab={:.2}s ({:.0}%) codegen+link={:.2}s ({:.0}%)",
            total.as_secs_f64(),
            elab_elapsed.as_secs_f64(),
            elab_pct,
            codegen_link.as_secs_f64(),
            codegen_pct,
        );
    }
    result
}

/// Apply compile flags to a CodeGen instance.
fn configure_codegen(
    codegen: &mut tungsten_codegen::CodeGen<'_>,
    file: &PathBuf,
    flags: &CompileFlags,
) {
    if flags.debug_info {
        let source_text = std::fs::read_to_string(file).unwrap_or_default();
        let source_path = file.to_string_lossy();
        codegen.enable_debug_info(&source_path, &source_text);
        if flags.verbose {
            eprintln!("Debug info enabled (definition-level DWARF)");
        }
    }
    if flags.diagnostics.codegen_backtrace {
        codegen.set_codegen_backtrace(true);
    }
    if let Some(ref filter) = flags.diagnostics.tracing.trace_adt_ops {
        codegen.set_trace_adt_ops(filter.clone());
    }
    if flags.diagnostics.tracing.trace_musttail {
        codegen.set_trace_musttail();
    }
    if flags.diagnostics.tracing.trace_escape {
        codegen.set_trace_escape();
    }
    if flags.named_lambdas {
        codegen.set_named_lambdas(true);
        if flags.verbose {
            eprintln!("Named lambdas enabled (source-level IR names)");
        }
    }
    if let Some(ref filter) = flags.diagnostics.alloc_profile {
        let filter_opt = if filter.is_empty() {
            None
        } else {
            Some(filter.clone())
        };
        codegen.set_alloc_profile(filter_opt);
        if flags.verbose {
            eprintln!("Allocation profiling enabled");
        }
    }
}

/// Initialize LLVM codegen, compile definitions, and emit output.
fn run_codegen(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    flags: &CompileFlags,
    project: driver::ProjectOutput,
    main_ty: &tungsten_core::Type,
) -> ExitCode {
    use tungsten_codegen::inkwell::context::Context as LlvmContext;
    use tungsten_codegen::CodeGen;

    // Initialize codegen
    let llvm_context = LlvmContext::create();
    let module_name = file.file_stem().unwrap_or_default().to_string_lossy();
    let mut codegen = CodeGen::new(&llvm_context, &module_name);

    configure_codegen(&mut codegen, file, flags);

    codegen.register_record_types(project.record_types);
    codegen.register_adt_types(convert_adt_types_for_codegen(project.adt_types));

    // Pass 1: Declare all functions and register definitions
    let extern_name_map = match declare_and_register_defs(&mut codegen, &project.defs) {
        Ok(map) => map,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Pass 2: Compile all definitions
    if let Err(e) = compile_all_defs(&mut codegen, &project.defs, &extern_name_map, flags.verbose) {
        eprintln!("error: {}", e);
        return ExitCode::FAILURE;
    }

    // Create main wrapper
    if flags.verbose {
        eprintln!("Creating main wrapper...");
    }
    if let Err(e) = codegen.compile_main_wrapper(main_ty) {
        eprintln!("error: could not create main wrapper: {}", e);
        return ExitCode::FAILURE;
    }

    // Finalize debug info before any code emission
    codegen.finalize_debug_info();

    // Emit output (LLVM IR or native executable)
    linking::emit_output(&codegen, file, output, flags)
}

// Re-export extern naming utilities for use by submodules.
