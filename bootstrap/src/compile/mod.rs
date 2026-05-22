//! `tungsten compile` — Compile to native executable via LLVM codegen.
//!
//! This module contains the compile command and its helpers:
//! - `cmd_compile()`: orchestrates elaboration → codegen → linking
//! - TyVar validation and post-elaboration cleanup
//! - Diagnostic helpers (dump-ir, dump-encoding)
//! - Type parameter substitution extraction

pub(crate) mod check_mono_coverage;
mod diagnostics;
mod extern_naming;
mod linking;
pub(crate) mod mangling;
pub(crate) mod mono;
pub(crate) mod per_module;
mod validation;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::{self, AdtTypes, CoreDef};

/// Compute the LLVM-level name for a definition.
///
/// Renames "main" to "tungsten_main" to avoid conflicts with the C main wrapper.
fn def_llvm_name(def_name: &str) -> String {
    if def_name == "main" {
        "tungsten_main".to_string()
    } else {
        def_name.to_string()
    }
}

/// Convert bootstrap ADT constructors to codegen-compatible constructors.
pub(crate) fn convert_adt_types_for_codegen(
    adt_types: AdtTypes,
) -> std::collections::HashMap<String, (Vec<String>, Vec<tungsten_codegen::CodegenConstructor>)> {
    adt_types
        .into_iter()
        .map(|(name, (params, constructors))| {
            let codegen_ctors: Vec<tungsten_codegen::CodegenConstructor> = constructors
                .into_iter()
                .map(|ctor| tungsten_codegen::CodegenConstructor {
                    name: ctor.name,
                    fields: ctor.fields,
                    index: ctor.index,
                })
                .collect();
            (name, (params, codegen_ctors))
        })
        .collect()
}

/// Pass 1: Declare all function prototypes and register definitions with codegen.
///
/// Returns the extern name mapping (original_name → LLVM name) on success.
pub(super) fn declare_and_register_defs<'ctx>(
    codegen: &mut tungsten_codegen::CodeGen<'ctx>,
    defs: &[CoreDef],
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut extern_name_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Declare all functions (add prototypes to enable forward references)
    for def in defs {
        let original_name = def_llvm_name(&def.name);

        let llvm_name = if let Some((orig, wrap)) = extern_wrap_name(&def.name, &def.term) {
            extern_name_map.insert(orig, wrap.clone());
            wrap
        } else {
            original_name.clone()
        };

        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            codegen.register_def_type(&llvm_name, &def.ty);
        } else if let Err(e) = codegen.declare_def(&llvm_name, &def.ty) {
            return Err(format!("declaration failed for '{}': {}", def.name, e));
        }
    }

    // Register extern name mappings with codegen for Global lookups
    codegen.register_extern_name_map(extern_name_map.clone());

    // Register term definitions for potential monomorphization
    for def in defs {
        let original_name = def_llvm_name(&def.name);
        let llvm_name = extern_name_map
            .get(&original_name)
            .cloned()
            .unwrap_or(original_name);
        codegen.register_term_def(&llvm_name, def.term.term.clone());
    }

    Ok(extern_name_map)
}

/// Pass 2: Compile all definitions into LLVM IR.
pub(super) fn compile_all_defs<'ctx>(
    codegen: &mut tungsten_codegen::CodeGen<'ctx>,
    defs: &[CoreDef],
    extern_name_map: &std::collections::HashMap<String, String>,
    verbose: bool,
) -> Result<(), String> {
    let total_defs = defs.len();
    for (i, def) in defs.iter().enumerate() {
        let original_name = def_llvm_name(&def.name);
        let llvm_name = extern_name_map
            .get(&original_name)
            .cloned()
            .unwrap_or(original_name);

        // Skip polymorphic definitions — compiled via monomorphization
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }

        if verbose {
            eprintln!("Compiling {} [{}/{}]...", def.name, i + 1, total_defs);
        }

        if let Err(e) = codegen.compile_def_with_span(
            &llvm_name,
            &def.term.term,
            &def.ty,
            def.term.span.map(|s| s.start),
        ) {
            return Err(format!("codegen failed for '{}': {}", def.name, e));
        }
    }
    Ok(())
}

/// Warn about definitions that still contain `sorry`.
pub(super) fn warn_sorry_defs(defs: &[CoreDef]) {
    let sorry_defs: Vec<_> = defs.iter().filter(|d| d.term.contains_sorry()).collect();
    if !sorry_defs.is_empty() {
        eprintln!(
            "warning: {} definition(s) contain `sorry` (may be dead code from pattern matching):",
            sorry_defs.len()
        );
        for def in &sorry_defs {
            eprintln!("  - {}", def.name);
        }
    }
}

/// Diagnostic and tracing flags for the compile command.
///
/// Groups optional diagnostic parameters that control IR dumps,
/// type tracing, and encoding inspection.
pub(crate) struct DiagnosticFlags {
    pub(crate) dump_ir: Option<String>,
    pub(crate) trace_types: Option<String>,
    pub(crate) dump_encoding: Option<String>,
    pub(crate) codegen_backtrace: bool,
    pub(crate) check_tyvar_escape: bool,
    /// Runtime and codegen tracing flags (musttail, escape, mono, ADT ops, etc.)
    pub(crate) tracing: TraceFlags,
    /// Allocation profiling filter (None = disabled, Some("") = all, Some(pat) = filtered).
    pub(crate) alloc_profile: Option<String>,
}

/// Runtime and codegen tracing toggles.
///
/// Grouped separately from `DiagnosticFlags` because these are all
/// simple on/off toggles that control stderr trace output during codegen.
pub(crate) struct TraceFlags {
    pub(crate) trace_adt_ops: Option<String>,
    pub(crate) trace_encoding: Option<String>,
    pub(crate) trace_normalization: Option<String>,
    pub(crate) trace_constructor_registration: bool,
    pub(crate) trace_musttail: bool,
    pub(crate) trace_escape: bool,
    pub(crate) trace_mono: bool,
}

/// Configuration flags for the compile command.
///
/// Bundles the boolean flags and diagnostic parameters that are
/// threaded through `cmd_compile` → `run_codegen` → `emit_output`.
pub(crate) struct CompileFlags {
    pub(crate) emit_llvm: bool,
    pub(crate) verbose: bool,
    pub(crate) max_errors: usize,
    pub(crate) dump_types: bool,
    pub(crate) debug_info: bool,
    pub(crate) sanitize: bool,
    pub(crate) named_lambdas: bool,
    pub(crate) no_codegen: bool,
    pub(crate) diagnostics: DiagnosticFlags,
    /// Number of parallel codegen jobs (default: num_cpus).
    /// Configured via TUNGSTEN_CODEGEN_JOBS env var.
    pub(crate) codegen_jobs: usize,
}

/// Parse `TUNGSTEN_CODEGEN_JOBS` env var (default: num_cpus, minimum: 1).
pub(crate) fn parse_codegen_jobs() -> usize {
    std::env::var("TUNGSTEN_CODEGEN_JOBS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.max(1))
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
}

/// Validate --no-codegen flag interactions.
/// Returns `Err(ExitCode::FAILURE)` if the combination is invalid (--emit-llvm).
/// Warns on codegen-only flags that will be ignored.
pub(super) fn validate_no_codegen_flags(
    flags: &CompileFlags,
    output: Option<&std::path::Path>,
) -> Result<(), ExitCode> {
    if flags.emit_llvm {
        eprintln!("error: --no-codegen and --emit-llvm are incompatible.");
        eprintln!("       --no-codegen stops before LLVM IR emission.");
        eprintln!("       Use --no-codegen alone, or --emit-llvm alone.");
        return Err(ExitCode::FAILURE);
    }
    if flags.debug_info {
        eprintln!("warning: ignored with --no-codegen: --debug-info");
    }
    if flags.sanitize {
        eprintln!("warning: ignored with --no-codegen: --sanitize");
    }
    if flags.diagnostics.codegen_backtrace {
        eprintln!("warning: ignored with --no-codegen: --codegen-backtrace");
    }
    if flags.diagnostics.tracing.trace_adt_ops.is_some() {
        eprintln!("warning: ignored with --no-codegen: --trace-adt-ops");
    }
    if flags.diagnostics.tracing.trace_musttail {
        eprintln!("warning: ignored with --no-codegen: --trace-musttail");
    }
    if flags.diagnostics.tracing.trace_escape {
        eprintln!("warning: ignored with --no-codegen: --trace-escape");
    }
    if flags.diagnostics.tracing.trace_mono {
        eprintln!("warning: ignored with --no-codegen: --trace-mono");
    }
    if output.is_some() {
        eprintln!("warning: ignored with --no-codegen: -o/--output");
    }
    Ok(())
}

/// Find the main function's type, or report a diagnostic error.
pub(super) fn find_main_type(
    file: &PathBuf,
    defs: &[CoreDef],
) -> Result<tungsten_core::Type, ExitCode> {
    use std::fs;
    let source = fs::read_to_string(file).unwrap_or_default();
    let eof_span = tungsten_bootstrap::span::Span::new(source.len() as u32, source.len() as u32);
    match defs.iter().find(|d| d.name == "main") {
        Some(d) => Ok(d.ty.clone()),
        None => {
            let err = tungsten_bootstrap::ElabError::no_main_function(eof_span);
            driver::render_diagnostics(&source, &file.to_string_lossy(), &[err], &[]);
            Err(ExitCode::FAILURE)
        }
    }
}

mod cmd_compile;
pub(crate) use cmd_compile::cmd_compile;

pub(crate) use extern_naming::{extern_wrap_name, get_extern_symbol};
