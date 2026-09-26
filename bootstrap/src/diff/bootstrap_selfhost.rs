//! `tungsten diff bootstrap-selfhost-check` — compare the bootstrap compiler and
//! the self-hosted compiler (tungsten1) on the same source.
//!
//! Runs the bootstrap (this binary) and a self-host binary (tungsten1) on the same file in check
//! mode, then compares error output. Cost 3+3 (two elaboration passes).
//! See ADR 20.5.26a.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, ExitCode};

/// Entry point for `tungsten diff bootstrap-selfhost-check <file> --selfhost-binary <path>`.
pub fn cmd_diff_bootstrap_selfhost_check(
    file: &Path,
    selfhost_binary: &Path,
    verbose: bool,
) -> ExitCode {
    if let Err(missing) = preflight(file, selfhost_binary) {
        match missing {
            MissingInput::SourceFile => {
                eprintln!("error: source file not found: {}", file.display());
            }
            MissingInput::SelfhostBinary => {
                eprintln!(
                    "error: self-host binary not found: {}",
                    selfhost_binary.display()
                );
                eprintln!("  hint: build with `make devcontainer-self-compile-fast`");
            }
        }
        return ExitCode::FAILURE;
    }

    println!(
        "Comparing bootstrap vs self-host check on {}...\n",
        file.display()
    );

    // --- Run the bootstrap (this binary) ---
    let bootstrap_exe = std::env::current_exe().unwrap_or_else(|_| "tungsten".into());
    let bootstrap_result = run_check(&bootstrap_exe, file, verbose);
    let (bootstrap_exit, bootstrap_errors) = match bootstrap_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to run the bootstrap: {e}");
            return ExitCode::FAILURE;
        }
    };

    // --- Run the self-host binary (tungsten1) ---
    let selfhost_result = run_check(selfhost_binary, file, verbose);
    let (selfhost_exit, selfhost_errors) = match selfhost_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to run the self-host binary: {e}");
            eprintln!("  hint: ensure `ulimit -s 65536` before running tungsten1");
            return ExitCode::FAILURE;
        }
    };

    // --- Compare ---
    println!("Results:");
    println!(
        "  bootstrap: exit={}, errors={}",
        bootstrap_exit,
        bootstrap_errors.len()
    );
    println!(
        "  self-host: exit={}, errors={}",
        selfhost_exit,
        selfhost_errors.len()
    );

    report_divergence(&bootstrap_errors, &selfhost_errors, verbose)
}

/// An input the comparison cannot start without.
#[derive(Debug, PartialEq, Eq)]
enum MissingInput {
    SourceFile,
    SelfhostBinary,
}

/// Both inputs must exist before either compiler is spawned. Returning *which*
/// one is missing (rather than printing and bailing inline) is what makes the
/// two guards distinguishable in a test — an exit code alone cannot tell them
/// apart, since both paths fail.
fn preflight(file: &Path, selfhost_binary: &Path) -> Result<(), MissingInput> {
    if !file.exists() {
        return Err(MissingInput::SourceFile);
    }
    if !selfhost_binary.exists() {
        return Err(MissingInput::SelfhostBinary);
    }
    Ok(())
}

/// Entries of the self-host-only list printed before the "and N more" tail.
const SELFHOST_LIST_CAP: usize = 20;
/// Entries of the bootstrap-only list printed under `--verbose`.
const BOOTSTRAP_LIST_CAP: usize = 10;

/// Report how the two error sets differ and pick the exit code: agreement (no
/// errors at all, or the identical set) succeeds; anything else is a
/// divergence worth a non-zero exit.
fn report_divergence(
    bootstrap_errors: &[String],
    selfhost_errors: &[String],
    verbose: bool,
) -> ExitCode {
    let (text, code) = render_report(bootstrap_errors, selfhost_errors, verbose);
    print!("{text}");
    code
}

/// Build the comparison report as text plus its exit code.
///
/// Rendering to a `String` instead of printing inline is deliberate: the
/// verdict, the list caps, and the `--verbose` gate are all *output* decisions,
/// so a test that only inspects the exit code cannot see any of them.
fn render_report(
    bootstrap_errors: &[String],
    selfhost_errors: &[String],
    verbose: bool,
) -> (String, ExitCode) {
    if bootstrap_errors.is_empty() && selfhost_errors.is_empty() {
        return (
            "\n✓ Both compilers agree: 0 errors\n".to_string(),
            ExitCode::SUCCESS,
        );
    }

    if bootstrap_errors == selfhost_errors {
        return (
            format!(
                "\n✓ Both compilers produce identical errors ({})\n",
                bootstrap_errors.len()
            ),
            ExitCode::SUCCESS,
        );
    }

    // Errors unique to the self-host binary are the regression signal; those
    // unique to the bootstrap are usually a self-host improvement or a
    // deliberate behavioural difference, so they only render under --verbose.
    let selfhost_only = errors_missing_from(selfhost_errors, bootstrap_errors);
    let bootstrap_only = errors_missing_from(bootstrap_errors, selfhost_errors);

    // Writing into a String is infallible, so each `write!` result is dropped.
    let mut out = String::new();
    if !selfhost_only.is_empty() {
        let _ = writeln!(
            out,
            "\n⚠ {} error(s) only in self-host (potential codegen regressions):",
            selfhost_only.len()
        );
        out.push_str(&numbered(&selfhost_only, SELFHOST_LIST_CAP));
        if selfhost_only.len() > SELFHOST_LIST_CAP {
            let _ = writeln!(
                out,
                "  ... and {} more",
                selfhost_only.len() - SELFHOST_LIST_CAP
            );
        }
    }

    if !bootstrap_only.is_empty() && verbose {
        let _ = writeln!(
            out,
            "\n  {} error(s) only in bootstrap (self-host may handle differently):",
            bootstrap_only.len()
        );
        out.push_str(&numbered(&bootstrap_only, BOOTSTRAP_LIST_CAP));
    }

    let _ = writeln!(
        out,
        "\nSummary: bootstrap={} errors, self-host={} errors, self-host-only={}, bootstrap-only={}",
        bootstrap_errors.len(),
        selfhost_errors.len(),
        selfhost_only.len(),
        bootstrap_only.len()
    );

    (out, ExitCode::FAILURE)
}

/// The entries of `theirs` that `mine` does not also contain.
fn errors_missing_from<'a>(theirs: &'a [String], mine: &[String]) -> Vec<&'a String> {
    theirs.iter().filter(|e| !mine.contains(e)).collect()
}

/// A 1-based numbered list, capped at `limit` entries. Whether a truncated list
/// gets an "and N more" tail is the caller's call — only the self-host-only
/// list (the regression signal) carries one.
fn numbered(errors: &[&String], limit: usize) -> String {
    let mut out = String::new();
    for (i, err) in errors.iter().enumerate().take(limit) {
        let _ = writeln!(out, "  {}. {}", i + 1, err);
    }
    out
}

/// True if a captured output line looks like a compiler error. Both spellings
/// count: `error[E0001]: …` (coded) and a bare `error: …` (uncoded).
fn is_error_line(line: &str) -> bool {
    line.contains("error[") || line.contains("error:")
}

/// Keep just the error lines from a run's captured streams, stderr first.
fn collect_error_lines(stderr: &str, stdout: &str) -> Vec<String> {
    stderr
        .lines()
        .chain(stdout.lines())
        .filter(|line| is_error_line(line))
        .map(str::to_string)
        .collect()
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

    Ok((exit_code, collect_error_lines(&stderr, &stdout)))
}

// Tests: bootstrap_selfhost/tests.rs
#[cfg(test)]
#[path = "bootstrap_selfhost/tests.rs"]
mod tests;
