//! `tungsten doctor check self-compile-readiness` — pre-flight checks for self-compile.
//!
//! Validates that the current platform and environment can successfully
//! self-compile a Tungsten binary. Catches issues that would otherwise
//! manifest as cryptic linker errors or runtime failures.
//!
//! Cost 1: filesystem + environment checks only, no parsing or elaboration.

use std::path::Path;
use std::process::{Command, ExitCode};

/// Run pre-flight checks for self-compile readiness.
pub fn cmd_check_self_compile_readiness(verbose: bool) -> ExitCode {
    let mut ok = true;
    let mut warnings = 0;

    eprintln!("Self-compile readiness checks:");

    // 1. Check filesystem case sensitivity
    match check_case_sensitivity(verbose) {
        CaseSensitivity::Sensitive => {
            if verbose {
                eprintln!("  filesystem: case-sensitive ✓");
            }
        }
        CaseSensitivity::Insensitive => {
            eprintln!("  filesystem: case-insensitive (macOS APFS or similar)");
            eprintln!("    note: object filenames use index prefixes to avoid collisions");
            warnings += 1;
        }
        CaseSensitivity::Unknown => {
            if verbose {
                eprintln!("  filesystem: could not determine case sensitivity");
            }
        }
    }

    // 2. Check C compiler availability
    match Command::new("cc").arg("--version").output() {
        Ok(output) if output.status.success() => {
            if verbose {
                let version = String::from_utf8_lossy(&output.stdout);
                let first_line = version.lines().next().unwrap_or("unknown");
                eprintln!("  cc: {} ✓", first_line);
            }
        }
        _ => {
            eprintln!("  FAIL: cc (C compiler) not found in PATH");
            ok = false;
        }
    }

    // 3. Check linker capabilities
    check_linker_capabilities(verbose, &mut warnings);

    // 4. Check LLVM availability
    match check_llvm(verbose) {
        Ok(()) => {}
        Err(msg) => {
            eprintln!("  FAIL: {}", msg);
            ok = false;
        }
    }

    // 5. Check static library availability
    check_static_lib(verbose, &mut ok);

    // Summary
    if ok && warnings == 0 {
        eprintln!("self-compile-readiness: all checks passed ✓");
    } else if ok {
        eprintln!(
            "self-compile-readiness: passed with {} warning(s)",
            warnings
        );
    } else {
        eprintln!("self-compile-readiness: FAILED — fix errors above before self-compiling");
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

enum CaseSensitivity {
    Sensitive,
    Insensitive,
    Unknown,
}

/// Check filesystem case sensitivity by creating temp files.
fn check_case_sensitivity(_verbose: bool) -> CaseSensitivity {
    use std::fs;

    let dir = std::env::temp_dir().join("tungsten_case_check");
    if fs::create_dir_all(&dir).is_err() {
        return CaseSensitivity::Unknown;
    }

    let lower = dir.join("case_test_a");
    let upper = dir.join("case_test_A");

    // Write to lowercase
    if fs::write(&lower, "lower").is_err() {
        let _ = fs::remove_dir_all(&dir);
        return CaseSensitivity::Unknown;
    }

    // Write to uppercase — if filesystem is case-insensitive, this overwrites
    if fs::write(&upper, "upper").is_err() {
        let _ = fs::remove_dir_all(&dir);
        return CaseSensitivity::Unknown;
    }

    // Read lowercase — if it now says "upper", filesystem is case-insensitive
    let result = match fs::read_to_string(&lower) {
        Ok(content) => {
            if content == "upper" {
                CaseSensitivity::Insensitive
            } else {
                CaseSensitivity::Sensitive
            }
        }
        Err(_) => CaseSensitivity::Unknown,
    };

    let _ = fs::remove_dir_all(&dir);
    result
}

/// Check linker capabilities and warn about known issues.
fn check_linker_capabilities(verbose: bool, warnings: &mut usize) {
    #[cfg(target_os = "macos")]
    {
        // On macOS, verify we're using system ld64 (not ld64.lld)
        eprintln!("  linker: macOS — using system ld64 (ld64.lld skipped for -stack_size support)");
        if verbose {
            if let Ok(output) = Command::new("ld").arg("-v").output() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let first_line = stderr.lines().next().unwrap_or("unknown");
                eprintln!("    ld version: {}", first_line);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Check if lld is available
        let lld_available = Command::new("cc")
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
            .unwrap_or(false);

        if lld_available {
            if verbose {
                eprintln!("  linker: lld available ✓");
            }
        } else {
            eprintln!("  linker: lld not available, using default linker");
            *warnings += 1;
        }
    }
}

/// Check LLVM availability via LLVM_SYS_180_PREFIX.
fn check_llvm(verbose: bool) -> Result<(), String> {
    if let Ok(prefix) = std::env::var("LLVM_SYS_180_PREFIX") {
        let llc = Path::new(&prefix).join("bin/llc");
        if llc.exists() {
            if verbose {
                eprintln!("  LLVM: {} ✓", prefix);
            }
            Ok(())
        } else {
            Err(format!(
                "LLVM_SYS_180_PREFIX={} but bin/llc not found",
                prefix
            ))
        }
    } else {
        // Check common locations
        #[cfg(target_os = "macos")]
        {
            let homebrew = "/opt/homebrew/opt/llvm@18";
            if Path::new(homebrew).join("bin/llc").exists() {
                if verbose {
                    eprintln!(
                        "  LLVM: found at {} (set LLVM_SYS_180_PREFIX to use)",
                        homebrew
                    );
                }
                return Ok(());
            }
        }
        Err("LLVM_SYS_180_PREFIX not set — needed for codegen".to_string())
    }
}

/// Check that libtungsten_core.a exists next to the compiler binary.
fn check_static_lib(verbose: bool, ok: &mut bool) {
    let lib_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("target/release"));

    let static_lib = lib_dir.join("libtungsten_core.a");
    if static_lib.exists() {
        if verbose {
            eprintln!("  libtungsten_core.a: {} ✓", static_lib.display());
        }
    } else {
        eprintln!(
            "  FAIL: libtungsten_core.a not found at {}",
            static_lib.display()
        );
        eprintln!("    hint: run `cargo build --release -p tungsten_core`");
        *ok = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_sensitivity_check_does_not_panic() {
        // Just verify the check runs without panicking
        let _ = check_case_sensitivity(false);
    }
}
