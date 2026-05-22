//! AddressSanitizer pipeline for compiled Tungsten programs.
//!
//! Routes through clang's ASan instrumentation instead of direct object
//! emission, so that ASan module+function passes are applied to the
//! generated LLVM IR.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use super::LinkerCommand;

/// Emit a sanitized binary by routing through clang's ASan pipeline.
///
/// Instead of direct object emission (which bypasses sanitizer passes), we:
/// 1. Emit LLVM IR to a temp file
/// 2. Use `clang -fsanitize=address` to compile+link in one step
///
/// This lets clang run the ASan module+function passes on the IR before
/// code generation, instrumenting all memory accesses in the Tungsten-generated code.
pub(super) fn emit_sanitized<'ctx>(
    codegen: &tungsten_codegen::CodeGen<'ctx>,
    file: &PathBuf,
    output: Option<&std::path::Path>,
    verbose: bool,
) -> ExitCode {
    use std::fs;

    let ir = codegen.get_ir_string();

    // Write IR to temp file
    let tmp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not create temp dir: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ll_path = tmp_dir.path().join("module.ll");
    if let Err(e) = fs::write(&ll_path, &ir) {
        eprintln!("error: could not write temp IR: {e}");
        return ExitCode::FAILURE;
    }

    let linker = LinkerCommand::new(file, output, true, verbose);

    // Use clang to compile IR with ASan instrumentation + link in one step.
    // clang applies ASan passes to the IR, instruments memory accesses, and
    // links against the ASan runtime automatically.
    // Links tungsten_core as a static archive (ADR 18.5.26e §2.3).
    let static_lib = linker.lib_dir.join("libtungsten_core.a");
    let mut cmd = Command::new("clang");
    cmd.arg("-fsanitize=address")
        .arg(&ll_path)
        .arg("-o")
        .arg(&linker.exe_path)
        .arg(&static_lib);

    #[cfg(target_os = "linux")]
    {
        cmd.args([
            "-lgcc_s",
            "-lutil",
            "-lrt",
            "-lpthread",
            "-lm",
            "-ldl",
            "-lc",
        ]);
    }
    #[cfg(target_os = "macos")]
    {
        cmd.args(["-lSystem", "-lc", "-lm"]);
    }

    if verbose {
        eprintln!(
            "ASan pipeline: clang -fsanitize=address {} → {}",
            ll_path.display(),
            linker.exe_path.display()
        );
    }

    match cmd.status() {
        Ok(s) if s.success() => {
            println!(
                "✓ Compiled to {} (AddressSanitizer enabled)",
                linker.exe_path.display()
            );
            ExitCode::SUCCESS
        }
        Ok(s) => {
            eprintln!("error: clang -fsanitize=address failed with status {s}");
            eprintln!("help: make sure 'clang' is installed and in PATH");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: could not run clang: {e}");
            eprintln!("help: ASan requires clang. Install with: brew install llvm (macOS) or apt install clang (Linux)");
            ExitCode::FAILURE
        }
    }
}
