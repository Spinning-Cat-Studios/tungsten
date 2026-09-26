//! Summary-line clauses and the assertion census row for `tungsten test`.
//!
//! Split out of `run.rs` for the 400-LOC file limit, along a seam that means
//! something: everything here is a **pure function from a count to a string**,
//! with no evaluator, no tally mutation and no I/O — which is exactly why the
//! wording is assertable without capturing stdout.
//!
//! Every clause is empty at zero. That is not cosmetic: it keeps an untripped
//! run's summary byte-identical to what the golden `.expected` files record.
//!
//! Tests: `bootstrap/src/test_runner/run/tests_asserted_nothing.rs`

/// The summary line's watchdog clause. Empty when nothing tripped, so a run
/// with no timeouts prints exactly the pre-21.7.26f summary line (the golden
/// `.expected` files depend on this).
pub(super) fn timed_out_clause(timed_out: usize) -> String {
    if timed_out == 0 {
        String::new()
    } else {
        format!("; {timed_out} timed out")
    }
}

/// The summary line's black-hole clause (ADR 22.7.26a), under the same
/// only-when-nonzero constraint as `timed_out_clause`: an untripped run's
/// summary stays byte-identical to the golden `.expected` files.
pub(super) fn black_holed_clause(black_holed: usize) -> String {
    if black_holed == 0 {
        String::new()
    } else {
        format!("; {black_holed} black-holed")
    }
}

/// The summary line's never-compared clause (ADR 1.8.26b D3), under the same
/// only-when-nonzero constraint as its two siblings.
pub(super) fn never_compared_clause(never_compared: usize) -> String {
    if never_compared == 0 {
        String::new()
    } else {
        format!("; {never_compared} never compared")
    }
}

/// One `--assertion-census` row (ADR 6.8.26b D4).
///
/// A pure function over the count rather than an inline `println!`, so the
/// wording — including the zero case, which is the one that matters — is
/// assertable without capturing stdout.
pub(super) fn census_line(assertions: u64) -> String {
    if assertions == 0 {
        "        0 assertion(s) executed — this test proves nothing".to_string()
    } else {
        format!("        {assertions} assertion(s) executed")
    }
}

/// The summary line's asserted-nothing clause (ADR 6.8.26b), under the same
/// only-when-nonzero constraint as its three siblings.
pub(super) fn asserted_nothing_clause(asserted_nothing: usize) -> String {
    if asserted_nothing == 0 {
        String::new()
    } else {
        format!("; {asserted_nothing} asserted nothing")
    }
}

/// The summary line's did-not-finish clause (ADR 6.8.26b D7), under the same
/// only-when-nonzero constraint as its four siblings.
pub(super) fn did_not_finish_clause(did_not_finish: usize) -> String {
    if did_not_finish == 0 {
        String::new()
    } else {
        format!("; {did_not_finish} did not finish")
    }
}

/// The summary line's expected-failure clause (ADR 6.8.26c D6), under the same
/// only-when-nonzero constraint as its five siblings.
pub(super) fn expected_failure_clause(expected_failures: usize) -> String {
    if expected_failures == 0 {
        String::new()
    } else {
        format!("; {expected_failures} expected to fail")
    }
}

// ---------------------------------------------------------------------------
// Hint text — the reader's next move under each failing outcome
// ---------------------------------------------------------------------------

/// The guidance printed under a TIMEOUT line — the reader's next move.
pub(super) const TIMEOUT_HINT: &str = "hint: non-terminating evaluation? `sample <pid>` a hung run, or see\n      docs/repo-memory/diagnostic-tools-cheatsheet.md (\"test appears hung\")";

/// The guidance printed under a BLACK HOLE line (ADR 22.7.26a) — same
/// next-move symmetry as `TIMEOUT_HINT`.
pub(super) const BLACK_HOLE_HINT: &str = "hint: the named global re-enters itself while being forced and has no\n      value (ADR 22.7.26a); see docs/repo-memory/diagnostic-tools-cheatsheet.md";

/// The guidance printed under a NEVER COMPARED line (ADR 1.8.26b D3). Points at
/// the cost-3 check that answers the same question *before* the tests are
/// written, which is the cheaper order.
pub(super) const NEVER_COMPARED_HINT: &str = "hint: this test's assertions did not execute, so it asserted nothing. Run\n      `tungsten doctor check comparable <T> <file>` (cost 3) before asserting\n      at a new type (ADR 1.8.26b).";

/// The asserted-nothing hint (ADR 6.8.26b). Points at the two causes that
/// reach a value without asserting — a stuck sub-expression (commonly an
/// extern the evaluator cannot execute) and a body that simply contains no
/// assertion — and at the census that says which tests are affected.
///
/// Deliberately names no *specific* extern. The original text named
/// `string_eq`/`tg_string_char_at_internal`, which D6 made executable in the
/// same change that shipped this hint — so it was false on arrival and stayed
/// false through 6.8.26c. The durable facts are the shape (an unexecutable
/// extern, reached indirectly) and the two commands that answer it; an
/// instance is by construction the part that goes stale.
pub(super) const ASSERTED_NOTHING_HINT: &str = "hint: the body finished but no assertion ran, so this test proves nothing.\n      Usually a sub-expression went Stuck — an `extern \"C\"` outside\n      `tungsten info eval externs` does that silently, and it is normally\n      reached INDIRECTLY, so the test body will not name it. Compare the two:\n      `tungsten info eval externs` against the call chain under the assertion.\n      Per-test counts: `tungsten test <file> --assertion-census` (ADR 6.8.26b).";

/// The did-not-finish hint (ADR 6.8.26b D7).
pub(super) const DID_NOT_FINISH_HINT: &str =
    "hint: the body stopped making progress, so every assertion after that point
      never ran. The evaluator returns a stuck term as a VALUE, which is why
      this used to report ok on a partial count.";

// ---------------------------------------------------------------------------
// Pure classification + rendering moved from run.rs (same seam: no I/O)
// ---------------------------------------------------------------------------

use super::TestOutcome;

/// Render a step count compactly for the TIMEOUT forensics line
/// (`4100000` → `4.1M`), so the magnitude reads at a glance.
///
/// Integer arithmetic throughout: a step count can exceed `f64`'s exact
/// integer range, and truncating rather than rounding keeps the figure from
/// overstating itself at a unit boundary (`999_999` reads `999.9K`, not `1000.0K`).
pub(super) fn format_step_count(steps: u64) -> String {
    match steps {
        0..=9_999 => steps.to_string(),
        10_000..=999_999 => format!("{}.{}K", steps / 1_000, (steps % 1_000) / 100),
        _ => format!("{}.{}M", steps / 1_000_000, (steps % 1_000_000) / 100_000),
    }
}

/// Classify a body that reached a value, from the three facts that decide it.
///
/// Pure over those facts rather than over the evaluator, so the **precedence**
/// is unit-testable — and the precedence is the part that is easy to get
/// backwards (ADR 6.8.26b D7):
///
/// 1. `Failed` first: a failing assertion is by definition one that ran.
/// 2. `AssertedNothing` next, NOT the residual check. `let` is strict, so a
///    wholly vacuous body arrives with *both* a zero count and a residual;
///    checking the residual first would report every such test as
///    `DidNotFinish` and bury the more useful diagnosis.
/// 3. `DidNotFinish` last: some assertions ran, then the body stopped, so any
///    later assertion never ran — partial vacuity the count cannot see.
pub(super) fn classify_finished_body(
    failed: bool,
    assertions: u64,
    reached_unit: bool,
) -> TestOutcome {
    if failed {
        TestOutcome::Failed("assertion failed".to_string())
    } else if assertions == 0 {
        TestOutcome::AssertedNothing
    } else if !reached_unit {
        TestOutcome::DidNotFinish { assertions }
    } else {
        TestOutcome::Passed
    }
}

/// Re-read an outcome through the manifest's expected-failure list (ADR
/// 6.8.26c D6), where `owner` is the successor ADR that will remove the entry.
///
/// Two rules, and the second is what stops the list rotting:
///
/// 1. A **failing** listed test becomes `ExpectedFailure` — reported, named,
///    and not gating. That is the ONLY sanctioned way to leave a finding
///    un-repaired; deleting the assertion, weakening its expected value and
///    demoting the file to tier 3 are all out of bounds.
/// 2. A **passing** listed test becomes `Failed`. The day the successor lands,
///    the gate says so instead of letting a stale entry sit green forever —
///    which is the failure mode every xfail list acquires without this rule.
///
/// `Skipped` is left alone: a tier-3 file executes nothing, so neither rule has
/// anything to say about it.
pub(super) fn apply_expected_failure(outcome: TestOutcome, owner: Option<&str>) -> TestOutcome {
    let Some(owner) = owner else {
        return outcome;
    };
    match outcome {
        TestOutcome::Skipped(_) => outcome,
        TestOutcome::Passed => TestOutcome::Failed(format!(
            "listed as an expected failure owned by ADR {owner}, but it PASSED. \
             If the successor has landed, delete its entry from the manifest — \
             a stale expected-failure entry silently un-gates a working test"
        )),
        failing => TestOutcome::ExpectedFailure {
            owner: owner.to_string(),
            reported: Box::new(failing),
        },
    }
}
