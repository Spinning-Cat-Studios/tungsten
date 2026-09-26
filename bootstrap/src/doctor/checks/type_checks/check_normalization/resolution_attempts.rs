//! Cross-run deferred-resolution-attempt determinism check
//! (ADR 23.7.26a §6.1 follow-up).
//!
//! `encoding-determinism` asks "does the driver produce the same stored
//! *results* twice?". This check asks the complementary question about the
//! *work*: "does deferred type-reference resolution make the same **number of
//! attempts** twice?". It elaborates the project twice under the
//! [`record_resolution_attempts`] instrumentation and compares the two
//! per-target-name attempt tallies.
//!
//! Motivation: ADR 23.7.26a §6.1 observed that the count of (mostly no-op)
//! deferred-reference resolutions can vary across processes while the stored
//! encodings stay byte-identical — an attempt-count nondeterminism that
//! `encoding-determinism` cannot see because it only compares results. Like
//! its sibling, the in-process two-run form is the fast canary (each run seeds
//! its `HashMap`s differently); for the strongest guarantee run `--json` in two
//! separate processes and `diff` the tallies.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use crate::elaborate::resolution_metrics::{record_resolution_attempts, ResolutionAttemptTally};

/// One target name's cross-run attempt-count comparison.
struct AttemptDivergence {
    name: String,
    run_a: Option<usize>,
    run_b: Option<usize>,
}

/// Run `doctor check type resolution-attempt-determinism`.
///
/// Elaborates `file` twice under attempt recording and reports any target
/// whose deferred-resolution-attempt count differs between the two runs. Exits
/// non-zero iff a divergence is found.
pub fn cmd_check_resolution_attempt_determinism(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    json: bool,
) -> ExitCode {
    let run_a = match elaborate_and_tally(file, verbose, max_errors) {
        Ok(tally) => tally,
        Err(code) => return code,
    };
    let run_b = match elaborate_and_tally(file, verbose, max_errors) {
        Ok(tally) => tally,
        Err(code) => return code,
    };

    let divergences = diff_attempt_tallies(&run_a, &run_b);

    // The report body is built by pure, unit-tested formatters; this command
    // only does the I/O (elaborate ×2, print, exit). The `-> ExitCode`
    // Default-mutant on this thin wrapper is intentionally unkillable — no test
    // can synthesize a *real* cross-run attempt divergence on a healthy build
    // (the same reasoning as `encoding-determinism`'s wrapper).
    print!(
        "{}",
        format_report(&run_a, &run_b, &divergences, verbose, json)
    );
    if divergences.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Elaborate `file` once with attempt recording on; return the tally, or the
/// failure `ExitCode` to propagate.
fn elaborate_and_tally(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> Result<ResolutionAttemptTally, ExitCode> {
    let (result, tally) =
        record_resolution_attempts(|| driver::elaborate_project(file, verbose, max_errors, None));
    match result {
        Ok(_) => Ok(tally),
        Err(e) => {
            eprintln!("error: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// Sum of attempts across every target — the headline per-run figure.
fn total_attempts(tally: &ResolutionAttemptTally) -> usize {
    tally.values().sum()
}

/// Compare two attempt tallies. Returns the targets whose counts differ (or
/// that appear in only one run), sorted by name.
fn diff_attempt_tallies(
    run_a: &ResolutionAttemptTally,
    run_b: &ResolutionAttemptTally,
) -> Vec<AttemptDivergence> {
    let mut divergences: Vec<AttemptDivergence> = Vec::new();
    for (name, &count_a) in run_a {
        match run_b.get(name) {
            Some(&count_b) if count_a == count_b => {}
            other => divergences.push(AttemptDivergence {
                name: name.clone(),
                run_a: Some(count_a),
                run_b: other.copied(),
            }),
        }
    }
    for (name, &count_b) in run_b {
        if !run_a.contains_key(name) {
            divergences.push(AttemptDivergence {
                name: name.clone(),
                run_a: None,
                run_b: Some(count_b),
            });
        }
    }
    divergences.sort_by(|x, y| x.name.cmp(&y.name));
    divergences
}

/// Targets ranked by run-A attempt count, highest first (the "counter" view).
fn ranked_by_attempts(tally: &ResolutionAttemptTally) -> Vec<(&str, usize)> {
    let mut ranked: Vec<(&str, usize)> = tally.iter().map(|(n, &c)| (n.as_str(), c)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ranked
}

/// Build the full report string for either output mode.
fn format_report(
    run_a: &ResolutionAttemptTally,
    run_b: &ResolutionAttemptTally,
    divergences: &[AttemptDivergence],
    verbose: bool,
    json: bool,
) -> String {
    if json {
        format_json(run_a, run_b, divergences)
    } else {
        format_human(run_a, run_b, divergences, verbose)
    }
}

fn format_human(
    run_a: &ResolutionAttemptTally,
    run_b: &ResolutionAttemptTally,
    divergences: &[AttemptDivergence],
    verbose: bool,
) -> String {
    let mut out = format!(
        "Checking deferred-resolution-attempt determinism across two runs...\n\n\
         run A: {} attempt(s) across {} target(s)\n\
         run B: {} attempt(s) across {} target(s)\n\n",
        total_attempts(run_a),
        run_a.len(),
        total_attempts(run_b),
        run_b.len(),
    );
    for divergence in divergences {
        out.push_str(&format!(
            "  ✗ {}: NON-DETERMINISTIC — run A {}, run B {}\n",
            divergence.name,
            count_label(divergence.run_a),
            count_label(divergence.run_b),
        ));
    }
    if verbose {
        out.push_str("\nAttempts by target (run A):\n");
        for (name, count) in ranked_by_attempts(run_a) {
            out.push_str(&format!("  {count:>6}  {name}\n"));
        }
    }
    out.push_str(&format!(
        "\nResult: {} target(s) with non-deterministic attempt counts\n",
        divergences.len()
    ));
    out
}

/// Render an optional per-run count (`absent` when a target appeared in only
/// the other run).
fn count_label(count: Option<usize>) -> String {
    match count {
        Some(n) => n.to_string(),
        None => "absent".to_string(),
    }
}

fn format_json(
    run_a: &ResolutionAttemptTally,
    run_b: &ResolutionAttemptTally,
    divergences: &[AttemptDivergence],
) -> String {
    let entries: Vec<serde_json::Value> = divergences
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "run_a": d.run_a,
                "run_b": d.run_b,
            })
        })
        .collect();
    let report = serde_json::json!({
        "run_a_total": total_attempts(run_a),
        "run_b_total": total_attempts(run_b),
        "run_a_targets": run_a.len(),
        "run_b_targets": run_b.len(),
        "non_deterministic": divergences.len(),
        "divergences": entries,
        "attempts_run_a": sorted_pairs(run_a),
    });
    serde_json::to_string_pretty(&report).unwrap()
}

/// Deterministically ordered `[name, count]` pairs for JSON output (a `HashMap`
/// would serialize in hash order, breaking the two-process `diff` recipe).
fn sorted_pairs(tally: &ResolutionAttemptTally) -> Vec<serde_json::Value> {
    ranked_by_attempts(tally)
        .into_iter()
        .map(|(name, count)| serde_json::json!([name, count]))
        .collect()
}

// Tests: resolution_attempts_tests.rs
#[cfg(test)]
#[path = "resolution_attempts_tests.rs"]
mod resolution_attempts_tests;
