//! `tungsten doctor self-test` — exercise the compiler on known-good programs.
//!
//! Runs each registered program through check → compile → run → verify,
//! reporting per-phase pass/fail with timing.
//! See ADR 16.4.26a §T4 and ADR 16.4.26b §2 for design rationale.

mod output;
mod registry;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use output::{print_json_results, print_test_result_text};
use registry::TEST_REGISTRY;

/// Tier determines which programs run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    /// Default: 3 core programs (hello, answer, option)
    Default,
    /// Full: all registered programs
    Full,
}

/// A single self-test entry.
struct TestEntry {
    name: &'static str,
    file: &'static str,
    expected_output: &'static str,
    tier: Tier,
}

/// Per-phase result within a single self-test.
#[derive(Debug)]
struct PhaseResult {
    phase: &'static str,
    passed: bool,
    duration_ms: u64,
    detail: String,
}

/// Result of running one self-test (with per-phase breakdown).
#[derive(Debug)]
struct TestResult {
    name: String,
    file: String,
    phases: Vec<PhaseResult>,
}

impl TestResult {
    fn passed(&self) -> bool {
        self.phases.iter().all(|p| p.passed)
    }

    fn total_ms(&self) -> u64 {
        self.phases.iter().map(|p| p.duration_ms).sum()
    }

    fn phase_count(&self) -> usize {
        self.phases.len()
    }

    fn passed_phase_count(&self) -> usize {
        self.phases.iter().filter(|p| p.passed).count()
    }
}

/// Find the workspace root by looking for Cargo.toml from current_exe's location.
fn find_workspace_root() -> Option<PathBuf> {
    // Try from the current exe location first (for installed binaries)
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("Cargo.toml").exists() && d.join("examples").is_dir() {
                return Some(d);
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    // Fall back to current working directory
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("Cargo.toml").exists() && cwd.join("examples").is_dir() {
            return Some(cwd);
        }
    }
    None
}

/// Resolve the tungsten compiler binary path.
fn find_compiler_binary() -> Option<PathBuf> {
    // Use current exe — self-test calls itself
    std::env::current_exe().ok()
}

/// Run a single phase, returning the phase result.
fn run_phase(phase: &'static str, cmd: &mut Command, verbose: bool) -> PhaseResult {
    if verbose {
        eprintln!("    [{phase}] {:?}", cmd);
    }
    let start = Instant::now();
    match cmd.output() {
        Err(e) => PhaseResult {
            phase,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            detail: format!("spawn failed: {e}"),
        },
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            PhaseResult {
                phase,
                passed: false,
                duration_ms: start.elapsed().as_millis() as u64,
                detail: format!("exit {:?}: {}", output.status.code(), stderr.trim()),
            }
        }
        Ok(output) => PhaseResult {
            phase,
            passed: true,
            duration_ms: start.elapsed().as_millis() as u64,
            detail: String::from_utf8_lossy(&output.stdout).to_string(),
        },
    }
}

/// Run a single self-test entry: check → compile → run → verify output.
///
/// Each phase stops on failure; remaining phases are skipped.
fn run_one_test(
    entry: &TestEntry,
    workspace_root: &Path,
    compiler: &Path,
    verbose: bool,
) -> TestResult {
    let source_path = workspace_root.join(entry.file);
    let mut result = TestResult {
        name: entry.name.to_string(),
        file: entry.file.to_string(),
        phases: Vec::with_capacity(3),
    };

    // Check source file exists
    if !source_path.exists() {
        result.phases.push(PhaseResult {
            phase: "check",
            passed: false,
            duration_ms: 0,
            detail: format!("source file not found: {}", source_path.display()),
        });
        return result;
    }

    // Phase 1: Check (parse + elaborate)
    let check_result = run_phase(
        "check",
        Command::new(compiler).arg("check").arg(&source_path),
        verbose,
    );
    let check_passed = check_result.passed;
    result.phases.push(check_result);
    if !check_passed {
        return result;
    }

    // Phase 2: Compile (codegen + link)
    let tmp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            result.phases.push(PhaseResult {
                phase: "compile",
                passed: false,
                duration_ms: 0,
                detail: format!("failed to create temp dir: {e}"),
            });
            return result;
        }
    };
    let output_path = tmp_dir.path().join(entry.name);

    let compile_result = run_phase(
        "compile",
        Command::new(compiler)
            .arg("compile")
            .arg(&source_path)
            .arg("-o")
            .arg(&output_path),
        verbose,
    );
    let compile_passed = compile_result.passed;
    result.phases.push(compile_result);
    if !compile_passed {
        return result;
    }

    // Phase 3: Run (execute + verify output)
    let run_result = run_phase("run", &mut Command::new(&output_path), verbose);
    let run_passed = run_result.passed;
    let run_stdout = run_result.detail.clone();
    result.phases.push(run_result);
    if !run_passed {
        return result;
    }

    // Verify output if expected_output is non-empty
    if !entry.expected_output.is_empty() && run_stdout != entry.expected_output {
        // Mark the run phase as failed with mismatch detail
        if let Some(last) = result.phases.last_mut() {
            last.passed = false;
            last.detail = format!(
                "output mismatch:\n  expected: {:?}\n  actual:   {:?}",
                entry.expected_output, run_stdout,
            );
        }
    }

    result
}

/// Run self-test suite.
pub(crate) fn cmd_self_test(full: bool, verbose: bool, json: bool) -> ExitCode {
    let tier = if full { Tier::Full } else { Tier::Default };

    // Resolve paths
    let workspace_root = match find_workspace_root() {
        Some(r) => r,
        None => {
            eprintln!("error: cannot find workspace root (need Cargo.toml + examples/)");
            return ExitCode::FAILURE;
        }
    };
    let compiler = match find_compiler_binary() {
        Some(c) => c,
        None => {
            eprintln!("error: cannot find tungsten compiler binary");
            return ExitCode::FAILURE;
        }
    };

    // Select programs to run
    let entries: Vec<&TestEntry> = TEST_REGISTRY
        .iter()
        .filter(|e| tier == Tier::Full || e.tier == Tier::Default)
        .collect();

    let tier_label = if full { "full" } else { "default" };
    if !json {
        eprintln!(
            "Running {} self-test(s) [tier: {}]...",
            entries.len(),
            tier_label,
        );
    }

    // Run each test
    let mut results: Vec<TestResult> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let result = run_one_test(entry, &workspace_root, &compiler, verbose);
        if !json {
            print_test_result_text(&result);
        }
        results.push(result);
    }

    // Summarize
    let total_phases: usize = results.iter().map(|r| r.phase_count()).sum();
    let passed_phases: usize = results.iter().map(|r| r.passed_phase_count()).sum();
    let programs_passed = results.iter().filter(|r| r.passed()).count();
    let programs_failed = results.iter().filter(|r| !r.passed()).count();
    let total_ms: u64 = results.iter().map(|r| r.total_ms()).sum();

    if json {
        print_json_results(&results, tier_label, total_ms);
    } else {
        eprintln!();
        eprintln!(
            "{} passed, {} failed ({} programs, {}/{} phases, {}ms)",
            programs_passed,
            programs_failed,
            results.len(),
            passed_phases,
            total_phases,
            total_ms,
        );
    }

    if programs_failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
