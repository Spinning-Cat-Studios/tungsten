//! Reporting for tests that executed no assertions (ADR 6.8.26b).
//!
//! A sibling of `tests_never_compared.rs`, split along the same seam and for
//! the same reason — but covering the route that one cannot. ADR 1.8.26b's
//! guard inspects an assertion's *operands*, so it fires only when the
//! assertion runs. When an enclosing expression goes `Stuck` first the
//! assertion is never reached, there are no operands, and the test used to
//! report `ok`. Counting executions catches that without knowing why.
//!
//! Tests: `bootstrap/src/test_runner/run/mod.rs` +
//! `bootstrap/src/test_runner/summary.rs`. The end-to-end halves — that a
//! vacuous body actually REACHES these outcomes, and that the run exits
//! nonzero — are `tests/test_runner_asserted_nothing.tg` and
//! `tests/test_runner_did_not_finish.tg`, driven by `make check-test-runner`.

use super::super::summary::classify_finished_body;
use super::super::summary::{asserted_nothing_clause, census_line, did_not_finish_clause};
use super::{report_outcome, Tally};
use crate::test_runner::TestOutcome;

// ---------------------------------------------------------------------------
// AssertedNothing — the zero-assertion outcome
// ---------------------------------------------------------------------------

/// The same three properties `NeverCompared` needs, for the same reason: a
/// test that asserted NOTHING must never count as passed, and folding it into
/// `Passed` is the whole defect. Each assertion fails under a different
/// mutation of the tally.
#[test]
fn an_asserted_nothing_outcome_tallies_separately_and_never_passes() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();

    report_outcome(
        "test_alpha",
        &TestOutcome::AssertedNothing,
        &mut tally,
        &mut failures,
        false,
    );

    assert_eq!(tally.asserted_nothing, 1);
    assert_eq!(
        tally.passed, 0,
        "a test that executed no assertion must never count as passed"
    );
    assert_eq!(tally.failed, 0, "it tallies apart from assertion failures");
    assert_eq!(tally.never_compared, 0, "and apart from 1.8.26b's outcome");
    assert_eq!(tally.did_not_finish, 0);
    assert_eq!(failures.len(), 1, "it must appear in the failures block");
    assert_eq!(failures[0].0, "test_alpha");
}

/// The exit code is what makes this a gate rather than a report.
#[test]
fn asserted_nothing_gates_the_exit_code() {
    let tally = Tally {
        asserted_nothing: 1,
        ..Tally::default()
    };
    assert!(
        tally.is_failing(),
        "a zero-assertion test must fail the run, else the gate is advisory"
    );
}

// ---------------------------------------------------------------------------
// DidNotFinish — the partial-vacuity outcome (D7)
// ---------------------------------------------------------------------------

/// The count alone cannot see this one: the test executed assertions, so the
/// count is nonzero, but the body stopped before reaching `Unit` and every
/// later assertion never ran.
#[test]
fn a_did_not_finish_outcome_tallies_separately_and_never_passes() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();

    report_outcome(
        "test_beta",
        &TestOutcome::DidNotFinish { assertions: 2 },
        &mut tally,
        &mut failures,
        false,
    );

    assert_eq!(tally.did_not_finish, 1);
    assert_eq!(
        tally.passed, 0,
        "a body that stopped mid-way must never count as passed"
    );
    assert_eq!(tally.asserted_nothing, 0, "it is a distinct diagnosis");
    assert_eq!(tally.failed, 0);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "test_beta");
    assert!(
        failures[0].1.contains('2'),
        "the failure text must carry how many assertions DID run, since that \
         is what distinguishes partial from total vacuity: {}",
        failures[0].1
    );
}

#[test]
fn did_not_finish_gates_the_exit_code() {
    let tally = Tally {
        did_not_finish: 1,
        ..Tally::default()
    };
    assert!(tally.is_failing());
}

// ---------------------------------------------------------------------------
// Summary clauses — silent when zero, so untripped runs stay byte-identical
// ---------------------------------------------------------------------------

/// Both clauses are empty at zero, which is what keeps the golden `.expected`
/// files valid for runs that trip neither.
#[test]
fn the_new_clauses_are_empty_when_nothing_tripped() {
    assert_eq!(asserted_nothing_clause(0), "");
    assert_eq!(did_not_finish_clause(0), "");
}

#[test]
fn the_new_clauses_name_their_counts_when_tripped() {
    assert_eq!(asserted_nothing_clause(3), "; 3 asserted nothing");
    assert_eq!(did_not_finish_clause(2), "; 2 did not finish");
}

// ---------------------------------------------------------------------------
// The census row (D4)
// ---------------------------------------------------------------------------

/// The zero row is the one that matters, so it says what zero *means* rather
/// than leaving the reader to infer it from a bare `0`.
#[test]
fn the_census_row_calls_out_a_zero_count() {
    let row = census_line(0);
    assert!(row.contains('0'), "{row}");
    assert!(
        row.contains("proves nothing"),
        "a bare `0 assertion(s)` reads as a stat; it must read as a verdict: {row}"
    );
}

#[test]
fn the_census_row_reports_a_nonzero_count_plainly() {
    let row = census_line(3);
    assert!(row.contains('3'), "{row}");
    assert!(
        !row.contains("proves nothing"),
        "a healthy row must not carry the warning: {row}"
    );
}

/// Distinct counts must render distinctly — the property that makes the census
/// worth printing at all, and the one a `-> Default` mutant would break.
#[test]
fn the_census_row_distinguishes_counts() {
    assert_ne!(census_line(1), census_line(2));
    assert_ne!(census_line(0), census_line(1));
}

// ---------------------------------------------------------------------------
// Outcome precedence (D7) — the part that is easy to get backwards
// ---------------------------------------------------------------------------

#[test]
fn a_failing_assertion_outranks_everything() {
    // A failed assertion is by definition one that ran, so the count is > 0.
    assert!(matches!(
        classify_finished_body(true, 1, true),
        TestOutcome::Failed(_)
    ));
}

/// The flagship case: `let` is strict, so a wholly vacuous body arrives with
/// BOTH a zero count and a residual. Checking the residual first would report
/// it `DidNotFinish` and bury the more useful diagnosis.
#[test]
fn zero_assertions_outranks_the_residual_check() {
    assert!(matches!(
        classify_finished_body(false, 0, false),
        TestOutcome::AssertedNothing
    ));
}

#[test]
fn a_residual_after_some_assertions_is_did_not_finish() {
    assert!(matches!(
        classify_finished_body(false, 2, false),
        TestOutcome::DidNotFinish { assertions: 2 }
    ));
}

#[test]
fn asserting_and_finishing_is_the_only_way_to_pass() {
    assert!(matches!(
        classify_finished_body(false, 1, true),
        TestOutcome::Passed
    ));
}
