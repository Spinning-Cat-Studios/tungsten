//! The expected-failure escape (ADR 6.8.26c D6).
//!
//! A sibling of `tests_asserted_nothing.rs`, and the counterweight to it: that
//! file asserts findings *gate*, this one asserts the one sanctioned way to
//! leave a finding un-gated — and that the permission cannot rot.
//!
//! The rot rule is the half that is easy to omit and the half that matters. An
//! xfail list without it accumulates entries for defects that were fixed years
//! ago, each one silently un-gating a working test.

use super::super::summary::{apply_expected_failure, expected_failure_clause};
use super::report::format_report;
use super::{report_outcome, Tally};
use crate::test_runner::TestOutcome;

/// Rule 1: a listed test that fails is permitted, named, and does NOT gate.
#[test]
fn a_listed_failing_test_is_permitted_and_does_not_gate() {
    let outcome = apply_expected_failure(TestOutcome::AssertedNothing, Some("7.8.26c"));
    let TestOutcome::ExpectedFailure { owner, reported } = &outcome else {
        panic!("a listed failing test must become ExpectedFailure, got {outcome:?}");
    };
    assert_eq!(owner, "7.8.26c");
    assert!(
        matches!(**reported, TestOutcome::AssertedNothing),
        "the underlying diagnosis must survive, or the reader learns only that \
         the test was allowed to fail and never how"
    );

    let mut tally = Tally::default();
    let mut failures = Vec::new();
    report_outcome("test_alpha", &outcome, &mut tally, &mut failures, false);

    assert_eq!(tally.expected_failures, 1);
    assert_eq!(tally.asserted_nothing, 0, "it must not double-count");
    assert_eq!(tally.passed, 0, "and it is certainly not a pass");
    assert!(failures.is_empty());
    assert!(
        !tally.is_failing(),
        "the whole point of the escape is that it does not gate"
    );
}

/// Rule 2 — the one that stops the list rotting. A listed test that PASSES is
/// a failure: the successor has landed and the entry must be deleted.
///
/// Without this, an entry outlives the defect it documents and silently
/// un-gates a working test for as long as nobody re-reads the manifest.
#[test]
fn a_listed_test_that_passes_fails_the_run() {
    let outcome = apply_expected_failure(TestOutcome::Passed, Some("7.8.26c"));
    let TestOutcome::Failed(reason) = &outcome else {
        panic!("a listed test that passes must FAIL, got {outcome:?}");
    };
    assert!(
        reason.contains("7.8.26c"),
        "the message must name the owning ADR so the reader knows what to delete"
    );

    let mut tally = Tally::default();
    let mut failures = Vec::new();
    report_outcome("test_alpha", &outcome, &mut tally, &mut failures, false);
    assert_eq!(tally.failed, 1);
    assert!(tally.is_failing());
}

/// An UNLISTED test is untouched in both polarities — otherwise the mechanism
/// would be rewriting outcomes it was never given permission over, and the two
/// tests above would pass for the wrong reason.
#[test]
fn an_unlisted_test_is_untouched_in_both_polarities() {
    assert!(matches!(
        apply_expected_failure(TestOutcome::Passed, None),
        TestOutcome::Passed
    ));
    assert!(matches!(
        apply_expected_failure(TestOutcome::AssertedNothing, None),
        TestOutcome::AssertedNothing
    ));
}

/// Every failing outcome is escapable, not just `AssertedNothing` — a defect
/// that manifests as a timeout or a black hole is no less deferrable, and a
/// mechanism that covered one shape would send its user looking for a worse
/// workaround for the others.
#[test]
fn every_failing_outcome_shape_is_escapable() {
    for outcome in [
        TestOutcome::Failed("assertion failed".to_string()),
        TestOutcome::AssertedNothing,
        TestOutcome::DidNotFinish { assertions: 2 },
        TestOutcome::TimedOut {
            secs: 60,
            steps: 10,
        },
        TestOutcome::BlackHole {
            cycle: vec!["f".to_string(), "f".to_string()],
        },
        TestOutcome::NeverCompared {
            reason: "no comparator".to_string(),
        },
    ] {
        assert!(
            matches!(
                apply_expected_failure(outcome, Some("7.8.26c")),
                TestOutcome::ExpectedFailure { .. }
            ),
            "every failing shape must be escapable"
        );
    }
}

/// A `Skipped` test is left alone: a tier-3 file executes nothing, so neither
/// rule has anything to say about it — and turning a skip into a rule-2
/// failure would make every tier-3 file with an entry unfixable.
#[test]
fn a_skipped_test_is_left_alone() {
    assert!(matches!(
        apply_expected_failure(
            TestOutcome::Skipped("check-only".to_string()),
            Some("7.8.26c")
        ),
        TestOutcome::Skipped(_)
    ));
}

/// The summary clause is empty at zero, like its five siblings — an untripped
/// run's summary line must stay byte-identical to what the golden `.expected`
/// files record.
#[test]
fn the_summary_clause_is_empty_at_zero_and_names_the_count_above_it() {
    assert_eq!(expected_failure_clause(0), "");
    assert_eq!(expected_failure_clause(1), "; 1 expected to fail");
    assert_eq!(expected_failure_clause(6), "; 6 expected to fail");
}

/// Every outcome renders under a distinct label. These are the words a reader
/// scans for, and an `ExpectedFailure` line quotes one to say *how* the test
/// failed — so a label that went empty or collided would make the escape
/// report that a test was permitted to fail without saying what it did.
#[test]
fn every_outcome_has_a_distinct_non_empty_label() {
    let labels = [
        TestOutcome::Passed.label(),
        TestOutcome::Failed(String::new()).label(),
        TestOutcome::Skipped(String::new()).label(),
        TestOutcome::TimedOut { secs: 1, steps: 1 }.label(),
        TestOutcome::BlackHole { cycle: vec![] }.label(),
        TestOutcome::NeverCompared {
            reason: String::new(),
        }
        .label(),
        TestOutcome::AssertedNothing.label(),
        TestOutcome::DidNotFinish { assertions: 1 }.label(),
        TestOutcome::ExpectedFailure {
            owner: String::new(),
            reported: Box::new(TestOutcome::Passed),
        }
        .label(),
    ];
    assert!(
        labels.iter().all(|l| !l.is_empty()),
        "an empty label prints a blank verdict"
    );
    let mut unique = labels.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        labels.len(),
        "two outcomes sharing a label are indistinguishable in the output"
    );
    assert_eq!(TestOutcome::AssertedNothing.label(), "ASSERTED NOTHING");
}

/// A skipped test tallies as skipped and nothing else — the tier-3 path, and
/// the one whose counter no other test in this crate asserts.
#[test]
fn a_skipped_test_tallies_only_as_skipped() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    report_outcome(
        "test_alpha",
        &TestOutcome::Skipped("check-only".to_string()),
        &mut tally,
        &mut failures,
        false,
    );
    assert_eq!(tally.skipped, 1);
    assert_eq!(tally.passed, 0, "a skipped test is not a passing one");
    assert_eq!(tally.failed, 0);
    assert!(failures.is_empty());
    assert!(!tally.is_failing(), "skips must not gate the exit code");
}

/// The summary line reports what actually happened, and the failures block
/// appears only when there are failures.
///
/// Asserted against the rendered text rather than trusting that printing
/// happened: a reporter that silently emitted nothing would be indexed as a
/// clean run by every reader, which is this ADR's own subject one level up.
#[test]
fn the_rendered_report_names_the_counts_and_the_failures() {
    let clean = format_report(
        &Tally {
            passed: 3,
            ..Tally::default()
        },
        &[],
        1.5,
        false,
    );
    assert!(clean.contains("result: ok. 3 passed; 0 failed; 0 skipped"));
    assert!(clean.contains("finished in 1.50s"));
    assert!(
        !clean.contains("failures:"),
        "a clean run must not print a failures block"
    );

    let dirty = format_report(
        &Tally {
            passed: 1,
            asserted_nothing: 2,
            expected_failures: 6,
            ..Tally::default()
        },
        &[("test_alpha", "it proved nothing".to_string())],
        0.25,
        false,
    );
    assert!(dirty.contains("result: FAILED."));
    assert!(dirty.contains("; 2 asserted nothing"));
    assert!(dirty.contains("; 6 expected to fail"));
    assert!(dirty.contains("failures:"));
    assert!(dirty.contains("test_alpha"));
    assert!(dirty.contains("it proved nothing"));
}
