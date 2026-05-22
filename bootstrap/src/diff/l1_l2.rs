//! `tungsten diff l1-l2-check` — compare L1 and tungsten1 elaboration results.
//!
//! Runs L1 (this binary) and an L2 binary (tungsten1) on the same file in check
//! mode, then compares error output. Cost 3+3 (two elaboration passes).
//! See ADR 20.5.26a.

use std::path::Path;
use std::process::{Command, ExitCode};

/// Entry point for `tungsten diff l1-l2-check <file> --l2-binary <path>`.
pub fn cmd_diff_l1_l2_check(file: &Path, l2_binary: &Path, verbose: bool) -> ExitCode {
    if !file.exists() {
        eprintln!("error: source file not found: {}", file.display());
        return ExitCode::FAILURE;
    }
    if !l2_binary.exists() {
        eprintln!("error: L2 binary not found: {}", l2_binary.display());
        eprintln!("  hint: build with `make devcontainer-self-compile-fast`");
        return ExitCode::FAILURE;
    }

    println!("Comparing L1 vs L2 check on {}...\n", file.display());

    // --- Run L1 (this binary) ---
    let l1_exe = std::env::current_exe().unwrap_or_else(|_| "tungsten".into());
    let l1_result = run_check(&l1_exe, file, verbose);
    let (l1_exit, l1_errors) = match l1_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to run L1: {e}");
            return ExitCode::FAILURE;
        }
    };

    // --- Run L2 (tungsten1) ---
    let l2_result = run_check(l2_binary, file, verbose);
    let (l2_exit, l2_errors) = match l2_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to run L2: {e}");
            eprintln!("  hint: ensure `ulimit -s 65536` before running tungsten1");
            return ExitCode::FAILURE;
        }
    };

    // --- Compare ---
    println!("Results:");
    println!("  L1: exit={}, errors={}", l1_exit, l1_errors.len());
    println!("  L2: exit={}, errors={}", l2_exit, l2_errors.len());

    if l1_errors.is_empty() && l2_errors.is_empty() {
        println!("\n✓ Both compilers agree: 0 errors");
        return ExitCode::SUCCESS;
    }

    if l1_errors == l2_errors {
        println!(
            "\n✓ Both compilers produce identical errors ({})",
            l1_errors.len()
        );
        return ExitCode::SUCCESS;
    }

    // Find errors unique to L2 (regressions)
    let l2_only: Vec<_> = l2_errors
        .iter()
        .filter(|e| !l1_errors.contains(e))
        .collect();
    // Find errors unique to L1 (L2 improvements or different behavior)
    let l1_only: Vec<_> = l1_errors
        .iter()
        .filter(|e| !l2_errors.contains(e))
        .collect();

    if !l2_only.is_empty() {
        println!(
            "\n⚠ {} error(s) only in L2 (potential codegen regressions):",
            l2_only.len()
        );
        for (i, err) in l2_only.iter().enumerate().take(20) {
            println!("  {}. {}", i + 1, err);
        }
        if l2_only.len() > 20 {
            println!("  ... and {} more", l2_only.len() - 20);
        }
    }

    if !l1_only.is_empty() && verbose {
        println!(
            "\n  {} error(s) only in L1 (L2 may handle differently):",
            l1_only.len()
        );
        for (i, err) in l1_only.iter().enumerate().take(10) {
            println!("  {}. {}", i + 1, err);
        }
    }

    println!(
        "\nSummary: L1={} errors, L2={} errors, L2-only={}, L1-only={}",
        l1_errors.len(),
        l2_errors.len(),
        l2_only.len(),
        l1_only.len()
    );

    ExitCode::FAILURE
}

/// Run `<binary> check <file>` and capture exit code + error lines.
fn run_check(binary: &Path, file: &Path, verbose: bool) -> Result<(i32, Vec<String>), String> {
    if verbose {
        eprintln!("  running: {} check {}", binary.display(), file.display());
    }

    let output = Command::new(binary)
        .arg("check")
        .arg(file)
        .output()
        .map_err(|e| format!("{}: {e}", binary.display()))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Collect error lines (lines containing "error[")
    let mut errors: Vec<String> = Vec::new();
    for line in stderr.lines().chain(stdout.lines()) {
        if line.contains("error[") || line.contains("error:") {
            errors.push(line.to_string());
        }
    }

    Ok((exit_code, errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_file_fails_gracefully() {
        let result = cmd_diff_l1_l2_check(
            Path::new("/nonexistent/test.tg"),
            Path::new("./tungsten1"),
            false,
        );
        assert_ne!(result, ExitCode::SUCCESS);
    }
}
