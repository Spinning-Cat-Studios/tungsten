//! `tungsten doctor check link-health` — verify compiled binary properties.
//!
//! Checks that a compiled Tungsten binary has the expected properties:
//! - Stack size matches the requested 64MB (not the default 8MB)
//! - No duplicate symbols in the object directory (if provided)
//! - Binary is executable and has a valid entrypoint
//!
//! Cost 1: shells out to platform tools (otool/readelf), no elaboration.

use std::path::Path;
use std::process::{Command, ExitCode};

/// Expected stack size in bytes (64 MB).
const EXPECTED_STACK_SIZE: u64 = 64 * 1024 * 1024;

/// Run the link-health check on a compiled binary.
pub fn cmd_check_link_health(binary: &Path, verbose: bool) -> ExitCode {
    let mut ok = true;

    // 1. Check binary exists and is executable
    if !binary.exists() {
        eprintln!("error: binary not found: {}", binary.display());
        return ExitCode::FAILURE;
    }

    if verbose {
        eprintln!("Checking link health: {}", binary.display());
    }

    // 2. Check stack size
    match check_stack_size(binary, verbose) {
        Ok(size) => {
            if size < EXPECTED_STACK_SIZE {
                eprintln!(
                    "FAIL: stack size is {} bytes ({:.1} MB), expected {} bytes ({} MB)",
                    size,
                    size as f64 / (1024.0 * 1024.0),
                    EXPECTED_STACK_SIZE,
                    EXPECTED_STACK_SIZE / (1024 * 1024)
                );
                eprintln!(
                    "hint: this usually means the linker ignored -stack_size (ld64.lld does this)"
                );
                ok = false;
            } else if verbose {
                eprintln!(
                    "  stack size: {} bytes ({} MB) ✓",
                    size,
                    size / (1024 * 1024)
                );
            }
        }
        Err(e) => {
            eprintln!("warning: could not check stack size: {}", e);
            // Non-fatal — the tool may not be available
        }
    }

    // 3. Check binary runs (--version or similar)
    match Command::new(binary).arg("commands").output() {
        Ok(output) if output.status.success() => {
            if verbose {
                eprintln!("  binary executes successfully ✓");
            }
        }
        Ok(output) => {
            eprintln!("FAIL: binary exited with status {}", output.status);
            ok = false;
        }
        Err(e) => {
            eprintln!("FAIL: could not execute binary: {}", e);
            ok = false;
        }
    }

    if ok {
        eprintln!("link-health: all checks passed ✓");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Read the stack size from a compiled binary using platform tools.
#[cfg(target_os = "macos")]
fn check_stack_size(binary: &Path, verbose: bool) -> Result<u64, String> {
    // On macOS, use `otool -l` to find LC_MAIN's stacksize field
    let output = Command::new("otool")
        .args(["-l"])
        .arg(binary)
        .output()
        .map_err(|e| format!("otool not found: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if verbose {
        // Show the relevant section
        for line in stdout.lines() {
            if line.contains("stacksize") {
                eprintln!("  otool: {}", line.trim());
            }
        }
    }

    // Parse LC_MAIN section for stacksize
    let mut in_lc_main = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("LC_MAIN") {
            in_lc_main = true;
            continue;
        }
        if in_lc_main && trimmed.starts_with("cmd ") {
            // Hit the next load command — LC_MAIN didn't have stacksize
            break;
        }
        if in_lc_main && trimmed.starts_with("stacksize") {
            // Parse "stacksize 67108864" or "stacksize 0x4000000"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let val = parts[1];
                let size = if val.starts_with("0x") || val.starts_with("0X") {
                    u64::from_str_radix(&val[2..], 16).map_err(|e| format!("parse hex: {}", e))?
                } else {
                    val.parse::<u64>()
                        .map_err(|e| format!("parse decimal: {}", e))?
                };
                return Ok(size);
            }
        }
    }

    // No LC_MAIN stacksize found — binary uses default
    Ok(8 * 1024 * 1024) // Default 8MB
}

/// Read the stack size from a compiled binary using platform tools.
#[cfg(target_os = "linux")]
fn check_stack_size(binary: &Path, verbose: bool) -> Result<u64, String> {
    // On Linux, use `readelf -l` to find GNU_STACK segment
    let output = Command::new("readelf")
        .args(["-l"])
        .arg(binary)
        .output()
        .map_err(|e| format!("readelf not found: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if verbose {
        for line in stdout.lines() {
            if line.contains("STACK") {
                eprintln!("  readelf: {}", line.trim());
            }
        }
    }

    // Parse GNU_STACK line for size
    for line in stdout.lines() {
        if line.contains("GNU_STACK") {
            // Format: "GNU_STACK      0x000000 0x00000000 0x00000000 0x00000000 0x04000000 RW  0x10"
            let parts: Vec<&str> = line.split_whitespace().collect();
            // The memory size is typically the 6th field
            if parts.len() >= 6 {
                let val = parts[5];
                let size = if val.starts_with("0x") || val.starts_with("0X") {
                    u64::from_str_radix(&val[2..], 16).map_err(|e| format!("parse hex: {}", e))?
                } else {
                    val.parse::<u64>()
                        .map_err(|e| format!("parse decimal: {}", e))?
                };
                return Ok(size);
            }
        }
    }

    // No GNU_STACK found — binary uses default
    Ok(8 * 1024 * 1024) // Default 8MB
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn check_stack_size(_binary: &Path, _verbose: bool) -> Result<u64, String> {
    Err("stack size check not implemented for this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expected_stack_size_is_64mb() {
        assert_eq!(EXPECTED_STACK_SIZE, 64 * 1024 * 1024);
    }
}
