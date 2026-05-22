//! Per-module codegen entry point: temp dir, emit-llvm, linking (ADR 7.5.26h).
//!
//! This module contains the top-level `run_codegen_per_module` function and its
//! helpers. The per-unit compilation logic lives in the parent `per_module::mod`.

use std::path::Path;
use std::path::PathBuf;

use tungsten_bootstrap::driver;

use super::run_per_module_codegen;
use crate::compile::CompileFlags;

/// Per-module codegen entry point: emit separate `.ll` files and link (ADR 7.5.26h).
///
/// Creates a temp directory, compiles each unit, handles `--emit-llvm`, and links.
/// With `--emit-llvm`, produces a mirror directory tree under `target/ll/` (or `-o <dir>`).
pub(in crate::compile) fn run_codegen_per_module(
    file: &PathBuf,
    output: Option<&Path>,
    flags: &CompileFlags,
    project: &driver::ProjectOutput,
    main_ty: &tungsten_core::types::Type,
) -> std::process::ExitCode {
    use std::process::ExitCode;

    // ADR 10.5.26j §2.2: Initialize Chrome trace layer when profile feature is active.
    // The guard must live through compilation + linking; dropping flushes the trace file.
    #[cfg(feature = "profile")]
    let _trace_guard = init_trace_subscriber();

    let tmp_dir = match create_codegen_temp_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };

    let modules = match run_per_module_codegen(file, flags, project, main_ty, tmp_dir.path()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if flags.emit_llvm {
        let output_dir = resolve_emit_llvm_dir(file, output);
        let source_root = file.parent().unwrap_or(Path::new("."));
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            eprintln!(
                "error: could not create output dir '{}': {}",
                output_dir.display(),
                e
            );
            return ExitCode::FAILURE;
        }
        // Copy .ll files into mirror directory structure (ADR 9.5.26b §2.4)
        // Per-function units: target/ll/<module_path>/<function_name>.ll
        // The modules list may include synthetic units (like __mono) that
        // aren't in project.codegen_units, so iterate modules directly.
        for module in &modules {
            if module.name == crate::compile::mono::MONO_DEPOT_UNIT {
                // Copy __mono.ll to output root
                let dest = output_dir.join(format!("{}.ll", module.name));
                if let Err(e) = std::fs::copy(&module.output_path, &dest) {
                    eprintln!("error: could not copy '{}': {}", dest.display(), e);
                    return ExitCode::FAILURE;
                }
                continue;
            }
            // Find the matching codegen unit for source path info
            let unit = match project.codegen_units.iter().find(|u| {
                let name = crate::compile::per_module::codegen_unit_name(
                    &u.source_file,
                    source_root,
                    &u.defs[0].name,
                );
                name == module.name
            }) {
                Some(u) => u,
                None => continue,
            };
            let relative_dir = match unit.source_file.strip_prefix(source_root) {
                Ok(r) => r.with_extension(""),
                Err(_) => {
                    eprintln!(
                        "error: source file '{}' is outside source root '{}'",
                        unit.source_file.display(),
                        source_root.display()
                    );
                    return ExitCode::FAILURE;
                }
            };
            let def_name = crate::compile::def_llvm_name(&unit.defs[0].name);
            let dest = output_dir
                .join(&relative_dir)
                .join(format!("{}.ll", def_name));
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("error: could not create dir '{}': {}", parent.display(), e);
                    return ExitCode::FAILURE;
                }
            }
            if let Err(e) = std::fs::copy(&module.output_path, &dest) {
                eprintln!("error: could not copy '{}': {}", dest.display(), e);
                return ExitCode::FAILURE;
            }
        }
        return crate::compile::linking::emit_per_module_ll(&modules, &output_dir, flags.verbose);
    }

    // Link compiled modules → binary
    let link_start = std::time::Instant::now();
    let result = crate::compile::linking::link_modules(&modules, file, output, flags);
    if flags.verbose {
        eprintln!("[perf] link={:.2}s", link_start.elapsed().as_secs_f64());
    }
    result
}

/// Create a temp directory for codegen intermediates (.ll, .o files).
fn create_codegen_temp_dir() -> Result<tempfile::TempDir, std::process::ExitCode> {
    tempfile::tempdir().map_err(|e| {
        eprintln!("error: could not create temp dir: {e}");
        std::process::ExitCode::FAILURE
    })
}

/// Determine the output directory for `--emit-llvm` .ll files (ADR 7.5.26h §2.3).
///
/// - If `-o` points to a file (has extension), use its parent directory.
/// - If `-o` points to a directory, use it directly.
/// - If neither is specified, default to `target/ll/`.
pub(in crate::compile) fn resolve_emit_llvm_dir(file: &PathBuf, output: Option<&Path>) -> PathBuf {
    match output {
        Some(p) if p.extension().is_some() => p.parent().unwrap_or(Path::new(".")).to_path_buf(),
        Some(p) => p.to_path_buf(),
        None => {
            // Default to target/ll/ relative to the entry file's parent directory
            let parent = file.parent().unwrap_or(Path::new("."));
            parent.join("target").join("ll")
        }
    }
}

/// Initialize the Chrome trace subscriber (ADR 10.5.26j §2.2).
///
/// Returns a flush guard that must live until the end of compilation.
/// Uses `try_init()` semantics so profile builds do not panic if another
/// subscriber has already been installed.
#[cfg(feature = "profile")]
fn init_trace_subscriber() -> Option<tracing_chrome::FlushGuard> {
    use tracing_chrome::ChromeLayerBuilder;
    use tracing_subscriber::prelude::*;

    let trace_path =
        std::env::var("TUNGSTEN_TRACE_FILE").unwrap_or_else(|_| "target/trace.json".to_string());
    let (chrome_layer, guard) = ChromeLayerBuilder::new()
        .file(&trace_path)
        .include_args(true)
        .build();
    match tracing_subscriber::registry().with(chrome_layer).try_init() {
        Ok(()) => {
            eprintln!("[profile] Tracing to {trace_path}");
            Some(guard)
        }
        Err(e) => {
            eprintln!("[profile] Could not initialize tracing: {e}");
            None
        }
    }
}

#[cfg(all(test, feature = "profile"))]
mod tests {
    #[test]
    fn test_init_trace_subscriber_does_not_panic_on_double_init() {
        // Point trace file at a real temp dir so ChromeLayerBuilder doesn't panic
        let tmp = tempfile::tempdir().expect("create temp dir");
        let trace_path = tmp.path().join("test-trace.json");
        std::env::set_var("TUNGSTEN_TRACE_FILE", trace_path.to_str().unwrap());

        let result = super::init_trace_subscriber();
        // First call may succeed or return None (if another test already init'd)
        // Either way, it must not panic.
        drop(result);

        // Second call should also not panic — try_init returns Err, we get None
        let second = super::init_trace_subscriber();
        assert!(
            second.is_none(),
            "double-init should return None, not panic"
        );

        std::env::remove_var("TUNGSTEN_TRACE_FILE");
    }
}
