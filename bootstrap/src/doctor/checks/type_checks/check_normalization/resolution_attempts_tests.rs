//! Tests for the deferred-resolution-attempt determinism check
//! (ADR 23.7.26a §6.1 follow-up).
//!
//! The comparator and formatters are pure, so they are unit-tested directly
//! (no elaboration). Two integration tests then elaborate a real tempdir
//! fixture: one asserts the command reports a clean fixture stable
//! (`ExitCode::SUCCESS`), the other asserts the instrumentation actually fires
//! during Phase-1d resolution (guarding the `note_resolution_attempt` call
//! sites — a deleted increment would leave the tally empty).

use super::*;
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;

fn tally(entries: &[(&str, usize)]) -> ResolutionAttemptTally {
    entries
        .iter()
        .map(|(name, count)| ((*name).to_string(), *count))
        .collect()
}

#[test]
fn identical_tallies_have_no_divergence() {
    let a = tally(&[("A", 3), ("B", 1)]);
    let b = a.clone();
    assert!(diff_attempt_tallies(&a, &b).is_empty());
}

#[test]
fn differing_count_is_divergent_with_both_sides() {
    // 1388 vs 1 — the exact §6.1 shape.
    let a = tally(&[("TypeDef", 1388)]);
    let b = tally(&[("TypeDef", 1)]);
    let divergences = diff_attempt_tallies(&a, &b);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].name, "TypeDef");
    assert_eq!(divergences[0].run_a, Some(1388));
    assert_eq!(divergences[0].run_b, Some(1));
}

#[test]
fn present_in_one_run_only_is_divergent() {
    let a = tally(&[("A", 2), ("Only", 5)]);
    let b = tally(&[("A", 2)]);
    let divergences = diff_attempt_tallies(&a, &b);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].name, "Only");
    assert_eq!(divergences[0].run_a, Some(5));
    assert_eq!(divergences[0].run_b, None);
}

#[test]
fn present_in_run_b_only_is_divergent() {
    // The second loop (b-only names) must also fire — kills a mutant that
    // drops it.
    let a = tally(&[("A", 2)]);
    let b = tally(&[("A", 2), ("OnlyB", 7)]);
    let divergences = diff_attempt_tallies(&a, &b);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].name, "OnlyB");
    assert_eq!(divergences[0].run_a, None);
    assert_eq!(divergences[0].run_b, Some(7));
}

#[test]
fn divergences_are_sorted_by_name() {
    let a = tally(&[("Zeta", 1), ("Alpha", 1), ("Mu", 1)]);
    let b = tally(&[("Zeta", 2), ("Alpha", 2), ("Mu", 2)]);
    let divergences = diff_attempt_tallies(&a, &b);
    let names: Vec<&str> = divergences.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Mu", "Zeta"]);
}

#[test]
fn total_attempts_sums_all_targets() {
    // Distinct non-1 values so a `+`→`*` or `sum`→0/1 mutant dies.
    assert_eq!(total_attempts(&tally(&[("A", 3), ("B", 4), ("C", 5)])), 12);
    assert_eq!(total_attempts(&tally(&[])), 0);
}

#[test]
fn ranked_by_attempts_is_count_desc_then_name_asc() {
    let t = tally(&[("low", 1), ("high", 100), ("mid_b", 10), ("mid_a", 10)]);
    let ranked = ranked_by_attempts(&t);
    assert_eq!(
        ranked,
        vec![("high", 100), ("mid_a", 10), ("mid_b", 10), ("low", 1)],
        "count descending, ties broken by name ascending"
    );
}

#[test]
fn count_label_renders_absent_for_none() {
    assert_eq!(count_label(Some(42)), "42");
    assert_eq!(count_label(None), "absent");
}

#[test]
fn format_human_reports_totals_divergences_and_verbose_table() {
    let a = tally(&[("Stable", 2), ("Flappy", 1388)]);
    let b = tally(&[("Stable", 2), ("Flappy", 1)]);
    let divergences = diff_attempt_tallies(&a, &b);

    let out = format_report(
        &a,
        &b,
        &divergences,
        /* verbose */ false,
        /* json */ false,
    );
    assert!(
        out.contains("run A: 1390 attempt(s) across 2 target(s)"),
        "run-A totals: {out}"
    );
    assert!(
        out.contains("run B: 3 attempt(s) across 2 target(s)"),
        "run-B totals: {out}"
    );
    assert!(
        out.contains("Flappy: NON-DETERMINISTIC — run A 1388, run B 1"),
        "names the divergent target with both counts: {out}"
    );
    assert!(
        out.contains("1 target(s) with non-deterministic attempt counts"),
        "result line: {out}"
    );
    // Non-verbose: no per-target table.
    assert!(
        !out.contains("Attempts by target"),
        "table is verbose-only: {out}"
    );

    let verbose = format_report(
        &a,
        &b,
        &divergences,
        /* verbose */ true,
        /* json */ false,
    );
    assert!(
        verbose.contains("Attempts by target (run A):"),
        "verbose table header: {verbose}"
    );
    assert!(
        verbose.contains("Flappy"),
        "verbose table lists targets: {verbose}"
    );
}

#[test]
fn format_report_json_dispatch_emits_sorted_json_not_human() {
    let a = tally(&[("A", 5), ("B", 3)]);
    let b = tally(&[("A", 5), ("B", 3)]);
    let divergences = diff_attempt_tallies(&a, &b);
    let out = format_report(&a, &b, &divergences, false, /* json */ true);

    assert!(out.trim_start().starts_with('{'), "must be JSON: {out}");
    assert!(
        out.contains("\"non_deterministic\": 0"),
        "stable JSON: {out}"
    );
    assert!(out.contains("\"run_a_total\": 8"), "totals in JSON: {out}");
    // attempts_run_a must be name-count pairs in ranked (deterministic) order,
    // so the two-process diff recipe is stable — "A" (5) before "B" (3).
    let a_pos = out.find("\"A\"").expect("A present");
    let b_pos = out.find("\"B\"").expect("B present");
    assert!(a_pos < b_pos, "ranked order (5 before 3) in JSON: {out}");
    assert!(
        !out.contains("Attempts by target"),
        "must not be the human report: {out}"
    );
}

/// A fixture whose ADT field references another named type, so Phase-1d
/// deferred resolution has something to resolve.
const CROSS_REF_FIXTURE: &str =
    "type Inner = I(Nat)\ntype Outer = O(Inner)\nfn main() -> Nat { 0 }";

/// Integration: the command reports a clean fixture stable (`SUCCESS`). Covers
/// the elaborate-twice wrapper end-to-end. (The divergent → `from(1)` branch is
/// unreachable on a healthy build — same untestable residual as the
/// `encoding-determinism` sibling.)
#[test]
fn command_reports_clean_fixture_stable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cross_ref.tg");
    fs::write(&path, CROSS_REF_FIXTURE).unwrap();
    let result = cmd_check_resolution_attempt_determinism(&path, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

/// Integration: recording captures a NON-empty tally during a real
/// elaboration — proving the `note_resolution_attempt` call sites in
/// `resolve_tyvars` actually fire (a deleted increment leaves this empty).
#[test]
fn recording_captures_attempts_during_real_elaboration() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cross_ref.tg");
    fs::write(&path, CROSS_REF_FIXTURE).unwrap();

    let (result, tally) =
        record_resolution_attempts(|| crate::driver::elaborate_project(&path, false, 20, None));
    assert!(result.is_ok(), "fixture must elaborate cleanly");
    assert!(
        total_attempts(&tally) > 0,
        "Phase-1d resolution must record at least one attempt, got {tally:?}"
    );
    assert!(
        tally.contains_key("Inner"),
        "the O(Inner) field reference must be resolved (keyed by target name), got {tally:?}"
    );
}

/// Integration: a file that fails to elaborate propagates non-success — the
/// command must NOT report a broken file "stable". This is the one input that
/// makes the wrapper return something OTHER than `SUCCESS` (via
/// `elaborate_and_tally`'s `Err`), so it is what kills the two wrapper mutants
/// a healthy-only SUCCESS test cannot: `cmd -> Default::default()` and
/// `elaborate_and_tally -> Ok(Default::default())` both collapse the broken
/// case to `SUCCESS`. (The `encoding-determinism` sibling allowlists the
/// equivalent mutant; this failure-path test kills it outright instead.)
#[test]
fn command_propagates_elaboration_failure() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("broken.tg");
    fs::write(&path, "type Color = | | |").unwrap();
    let result = cmd_check_resolution_attempt_determinism(&path, false, 20, false);
    assert_ne!(
        result,
        ExitCode::SUCCESS,
        "a file that fails to elaborate must not report stable"
    );
}
