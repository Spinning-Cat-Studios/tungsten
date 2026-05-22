//! Linking and output emission for compiled Tungsten programs.
//!
//! Handles emitting LLVM IR files, writing object files, linking native
//! executables, the AddressSanitizer pipeline via clang, and per-module
//! `.ll` → `.o` → binary linking (ADR 6.5.26c §2.7).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use super::per_module::CompiledModule;
use super::CompileFlags;

mod sanitize;

/// Shared linker configuration, extracted from `link_executable`, `link_object_files`,
/// and `emit_sanitized` to eliminate duplication (ADR 10.5.26a §2.2).
pub(super) struct LinkerCommand {
    pub(super) exe_path: PathBuf,
    pub(super) lib_dir: PathBuf,
    pub(super) sanitize: bool,
    pub(super) verbose: bool,
}

impl LinkerCommand {
    pub(super) fn new(file: &Path, output: Option<&Path>, sanitize: bool, verbose: bool) -> Self {
        let exe_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            #[cfg(target_os = "windows")]
            {
                file.with_extension("exe")
            }
            #[cfg(not(target_os = "windows"))]
            {
                file.with_extension("")
            }
        });
        let lib_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("target/release"));
        Self {
            exe_path,
            lib_dir,
            sanitize,
            verbose,
        }
    }

    /// Build a `cc` command with common flags, accepting input files separately.
    ///
    /// Inputs are added in the order provided — callers are responsible for
    /// sorting object files for deterministic linking.
    /// Uses `cc -fuse-ld=lld` when `lld` is available (ADR 9.5.26d §2.4).
    pub(super) fn build_cc(&self, inputs: &[PathBuf]) -> Command {
        let mut cmd = Command::new("cc");

        // Prefer lld when available (ADR 9.5.26d §2.4).
        // Skip on macOS: ld64.lld's MachO backend has incomplete support —
        // notably -stack_size is unimplemented (silently ignored), which
        // causes stack overflows in large programs. Apple's system ld64
        // handles -stack_size correctly.
        #[cfg(not(target_os = "macos"))]
        if lld_available() {
            cmd.arg("-fuse-ld=lld");
        }

        for input in inputs {
            cmd.arg(input);
        }
        cmd.arg("-o").arg(&self.exe_path);

        // Link tungsten_core as a static archive (ADR 18.5.26e §2.2).
        // Use the full path to the .a file to avoid accidental dynamic linking.
        let static_lib = self.lib_dir.join("libtungsten_core.a");
        cmd.arg(&static_lib);

        // Rust's staticlib does not bundle its transitive C dependencies the way
        // cdylib does. Add them explicitly per platform.
        // Authoritative source: `rustc --print native-static-libs --crate-type staticlib`
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
            // Set stack size to 128 MB — elaboration of large programs (e.g., the
            // compiler checking itself) can exceed the default 8 MB.
            // See ADR 20.5.26e (tail-recursive list operations).
            cmd.args(["-Wl,-z,stack-size=134217728"]);
        }
        #[cfg(target_os = "macos")]
        {
            cmd.args(["-lSystem", "-lc", "-lm"]);
            // Set stack size to 128 MB — elaboration of large programs (e.g., the
            // compiler checking itself) can exceed the default 8 MB.
            // See ADR 20.5.26e (tail-recursive list operations).
            cmd.args(["-Wl,-stack_size,0x8000000"]);
        }

        if self.sanitize {
            cmd.arg("-fsanitize=address");
        }
        cmd
    }

    /// Run the linker command and return the exit code, logging as appropriate.
    pub(super) fn run_link(&self, mut cmd: Command) -> ExitCode {
        if self.verbose {
            eprintln!("Library directory: {}", self.lib_dir.display());
        }
        match cmd.status() {
            Ok(s) if s.success() => ExitCode::SUCCESS,
            Ok(s) => {
                eprintln!("error: linker failed with status {}", s);
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("error: could not run linker: {}", e);
                eprintln!("help: make sure 'cc' (C compiler) is installed and in PATH");
                ExitCode::FAILURE
            }
        }
    }
}

/// Check if `lld` is available via `cc -fuse-ld=lld` (ADR 9.5.26d §2.4).
///
/// Probes once per process via a cached result. Falls back to default `cc`
/// linking when `lld` is not installed.
///
/// Skipped on macOS: ld64.lld has incomplete MachO support (e.g., -stack_size
/// is unimplemented). We always use Apple's system ld64 there.
#[cfg(not(target_os = "macos"))]
fn lld_available() -> bool {
    use std::sync::OnceLock;
    static LLD_AVAILABLE: OnceLock<bool> = OnceLock::new();
    *LLD_AVAILABLE.get_or_init(|| {
        // Use -nostdlib -nostartfiles to avoid "undefined symbol: main" from
        // empty input. We only need to check that `cc` accepts `-fuse-ld=lld`
        // and can invoke lld without error.
        Command::new("cc")
            .args([
                "-fuse-ld=lld",
                "-nostdlib",
                "-nostartfiles",
                "-shared",
                "-x",
                "c",
                "-",
                "-o",
                "/dev/null",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Emit compiled output: LLVM IR file or native executable (object + link).
pub(super) fn emit_output<'ctx>(
    codegen: &tungsten_codegen::CodeGen<'ctx>,
    file: &PathBuf,
    output: Option<&std::path::Path>,
    flags: &CompileFlags,
) -> ExitCode {
    use std::fs;

    if flags.emit_llvm {
        let ir = codegen.get_ir_string();
        let output_path = output
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| file.with_extension("ll"));

        if let Err(e) = fs::write(&output_path, ir) {
            eprintln!("error: could not write '{}': {}", output_path.display(), e);
            return ExitCode::from(3);
        }

        println!("✓ Wrote LLVM IR to {}", output_path.display());
        return ExitCode::SUCCESS;
    }

    // Write object file
    if flags.verbose {
        eprintln!("Writing object file...");
    }

    // When --sanitize is enabled, go through an IR→clang path so that clang
    // can apply ASan instrumentation passes to the generated LLVM IR.
    // Direct object emission skips ASan passes, defeating the purpose.
    if flags.sanitize {
        return sanitize::emit_sanitized(codegen, file, output, flags.verbose);
    }

    let obj_path = file.with_extension("o");
    if let Err(e) = codegen.write_object_file(&obj_path) {
        eprintln!("error: could not write object file: {}", e);
        return ExitCode::FAILURE;
    }

    if flags.verbose {
        eprintln!("Wrote object file to {}", obj_path.display());
    }

    // Link with system linker
    link_executable(file, output, &obj_path, flags.verbose, flags.sanitize)
}

/// Link an object file into an executable using the system linker.
fn link_executable(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    obj_path: &std::path::Path,
    verbose: bool,
    sanitize: bool,
) -> ExitCode {
    use std::fs;

    let linker = LinkerCommand::new(file, output, sanitize, verbose);
    let inputs = vec![obj_path.to_path_buf()];
    let cmd = linker.build_cc(&inputs);

    if verbose && sanitize {
        eprintln!("AddressSanitizer enabled: linking with -fsanitize=address");
    }

    let result = linker.run_link(cmd);

    // Clean up object file
    let _ = fs::remove_file(obj_path);

    if result == ExitCode::SUCCESS {
        println!("✓ Compiled to {}", linker.exe_path.display());
    }
    result
}

/// Link multiple compiled modules into a single executable (ADR 6.5.26c §2.7).
///
/// When modules are `.o` (in-process object emission, ADR 9.5.26e §2.1),
/// skips `llc` assembly and links directly. When modules are `.ll`, runs
/// `llc` on each to produce `.o` first.
pub(super) fn link_modules(
    modules: &[CompiledModule],
    file: &PathBuf,
    output: Option<&std::path::Path>,
    flags: &CompileFlags,
) -> ExitCode {
    use super::per_module::OutputKind;

    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("link").entered();

    // Step 1: Get .o paths — either directly (Obj) or via llc assembly (Ll)
    let all_obj = modules.iter().all(|m| m.kind == OutputKind::Obj);
    let obj_paths = if all_obj {
        // In-process .o emission: modules are already .o, skip llc entirely
        modules.iter().map(|m| m.output_path.clone()).collect()
    } else {
        match assemble_modules(modules, flags.verbose) {
            Ok(paths) => paths,
            Err(code) => return code,
        }
    };

    // Step 2: Sort object paths for deterministic link order (ADR 10.5.26a §2.2).
    let mut obj_paths = obj_paths;
    obj_paths.sort();
    link_object_files(&obj_paths, file, output, flags)
}

/// Run `llc` on each module's `.ll` file to produce `.o` files.
fn assemble_modules(modules: &[CompiledModule], verbose: bool) -> Result<Vec<PathBuf>, ExitCode> {
    use std::process::Command;

    let mut obj_paths = Vec::new();
    for module in modules {
        let obj_path = module.output_path.with_extension("o");

        let mut llc_cmd = Command::new("llc");
        llc_cmd
            .arg("-filetype=obj")
            .arg(&module.output_path)
            .arg("-o")
            .arg(&obj_path);

        if verbose {
            eprintln!(
                "  llc {} → {}",
                module.output_path.display(),
                obj_path.display()
            );
        }

        match llc_cmd.status() {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!(
                    "error: llc failed for module '{}' with status {}",
                    module.name, s
                );
                return Err(ExitCode::FAILURE);
            }
            Err(e) => {
                eprintln!("error: could not run llc: {}", e);
                eprintln!("help: make sure 'llc' is installed and in PATH");
                return Err(ExitCode::FAILURE);
            }
        }
        obj_paths.push(obj_path);
    }
    Ok(obj_paths)
}

/// Link multiple `.o` files into an executable.
fn link_object_files(
    obj_paths: &[PathBuf],
    file: &PathBuf,
    output: Option<&std::path::Path>,
    flags: &CompileFlags,
) -> ExitCode {
    use std::fs;

    let linker = LinkerCommand::new(file, output, flags.sanitize, flags.verbose);
    let cmd = linker.build_cc(obj_paths);

    if flags.verbose {
        eprintln!(
            "Linking {} object file(s) → {}",
            obj_paths.len(),
            linker.exe_path.display()
        );
    }

    let result = linker.run_link(cmd);

    for obj_path in obj_paths {
        let _ = fs::remove_file(obj_path);
    }

    if result == ExitCode::SUCCESS {
        println!("✓ Compiled to {}", linker.exe_path.display());
    }
    result
}

/// Emit per-module `.ll` files (for --emit-llvm with per-module codegen).
pub(super) fn emit_per_module_ll(
    modules: &[CompiledModule],
    output_dir: &Path,
    verbose: bool,
) -> ExitCode {
    for module in modules {
        if verbose {
            eprintln!("  {}", module.output_path.display());
        }
    }
    println!(
        "✓ Wrote {} LLVM IR file(s) to {}",
        modules.len(),
        output_dir.display()
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
