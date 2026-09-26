//! Reporting for comparisons that never ran (ADR 1.8.26b D3).
//!
//! A sibling of `tests_watchdog.rs` for the file-size limit, split along the
//! seam that matters: the watchdog and black-hole outcomes answer "this test
//! did not finish", and these answer "this test finished having asserted
//! nothing" — the outcome that used to be reported `ok`.
//!
//! Tests: bootstrap/src/test_runner/run.rs

use super::super::summary::never_compared_clause;
use super::{report_outcome, Tally};
use crate::test_runner::TestOutcome;

// The same three properties the black-hole outcome needs, for the same reason:
// a comparison that never ran is a test that asserted NOTHING, and folding it
// into `Passed` — which is where it landed before the gate — is the whole
// defect. Each assertion below fails under a different mutation of the tally.

#[test]
fn a_never_compared_outcome_tallies_separately_and_records_the_reason() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let outcome = TestOutcome::NeverCompared {
        reason: "cannot compare `Alpha`: no comparator could be synthesized".to_string(),
    };

    report_outcome("test_alpha", &outcome, &mut tally, &mut failures, false);

    assert_eq!(tally.never_compared, 1);
    assert_eq!(
        tally.passed, 0,
        "a test that asserted nothing must never count as passed"
    );
    assert_eq!(
        tally.failed, 0,
        "never-compared tallies apart from assertion failures"
    );
    assert_eq!(tally.timed_out, 0);
    assert_eq!(tally.black_holed, 0);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "test_alpha");
    assert!(
        failures[0].1.contains("Alpha"),
        "detail must name the type: {}",
        failures[0].1
    );
}

#[test]
fn a_never_compared_result_alone_makes_the_run_fail() {
    // The exit-code arm. Without it the runner reports the outcome and still
    // exits 0 — which is indistinguishable, to CI, from the silent pass this
    // ADR removed.
    let tally = Tally {
        passed: 3,
        never_compared: 1,
        ..Default::default()
    };
    assert!(tally.is_failing());
}

#[test]
fn no_never_compared_results_produce_an_empty_summary_clause() {
    // Same zero-noise constraint as the two clauses above: an untripped run's
    // summary line stays byte-identical, so the golden .expected files stand.
    assert_eq!(never_compared_clause(0), "");
}

#[test]
fn never_compared_results_are_named_in_the_summary_clause() {
    assert_eq!(never_compared_clause(1), "; 1 never compared");
    assert_eq!(never_compared_clause(2), "; 2 never compared");
}
