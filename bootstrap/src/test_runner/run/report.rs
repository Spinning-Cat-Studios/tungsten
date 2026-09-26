//! Per-test reporting and the run tally for `tungsten test`.
//!
//! Split out of `run/mod.rs` for the 400-LOC file limit, along the seam that
//! means something: everything here turns an already-decided `TestOutcome`
//! into stdout and a tally. Nothing here evaluates, and nothing here decides
//! what an outcome *is* — `summary::classify_finished_body` and
//! `summary::apply_expected_failure` own that, and are pure.

use super::super::summary::{
    asserted_nothing_clause, black_holed_clause, did_not_finish_clause, expected_failure_clause,
    format_step_count, never_compared_clause, timed_out_clause, ASSERTED_NOTHING_HINT,
    BLACK_HOLE_HINT, DID_NOT_FINISH_HINT, NEVER_COMPARED_HINT, TIMEOUT_HINT,
};
use super::super::{paint, Style, TestOutcome};

/// Running tallies while iterating tests.
#[derive(Default)]
pub(super) struct Tally {
    pub(super) passed: usize,
    pub(super) failed: usize,
    pub(super) skipped: usize,
    /// Watchdog trips. Counted separately from `failed` so the summary can name
    /// them, but they gate the exit code exactly like a failure.
    pub(super) timed_out: usize,
    /// Black holes (ADR 22.7.26a): a global re-entered its own forcing.
    /// Counted separately from both `failed` and `timed_out`, gating the exit
    /// code exactly like a failure.
    pub(super) black_holed: usize,
    /// Comparisons that never ran (ADR 1.8.26b D3). Counted separately for the
    /// same reason as the two above, and gating the exit code identically.
    pub(super) never_compared: usize,
    /// Tests that finished having executed no assertion (ADR 6.8.26b).
    /// Counted separately for the same reason as the three above, and gating
    /// the exit code identically.
    pub(super) asserted_nothing: usize,
    /// Tests that stopped making progress mid-body (ADR 6.8.26b D7).
    pub(super) did_not_finish: usize,
    /// Tests the manifest permits to fail (ADR 6.8.26c D6). The ONE tally that
    /// deliberately does NOT gate the exit code — every entry names the
    /// successor ADR that will remove it, and a listed test that starts passing
    /// is re-reported as `failed`, so the permission cannot rot green.
    pub(super) expected_failures: usize,
}

impl Tally {
    /// Whether the run should exit nonzero.
    pub(super) fn is_failing(&self) -> bool {
        self.failed > 0
            || self.timed_out > 0
            || self.black_holed > 0
            || self.never_compared > 0
            || self.asserted_nothing > 0
            || self.did_not_finish > 0
    }
}

/// Print a single test's line and update the tally / failures list.
pub(super) fn report_outcome<'a>(
    name: &'a str,
    outcome: &TestOutcome,
    tally: &mut Tally,
    failures: &mut Vec<(&'a str, String)>,
    use_color: bool,
) {
    match outcome {
        TestOutcome::Passed => {
            println!("test {} ... {}", name, paint("ok", Style::Green, use_color));
            tally.passed += 1;
        }
        TestOutcome::Failed(msg) => {
            println!(
                "test {} ... {}",
                name,
                paint("FAILED", Style::Red, use_color)
            );
            failures.push((name, msg.clone()));
            tally.failed += 1;
        }
        TestOutcome::Skipped(reason) => {
            println!(
                "test {} ... {} ({})",
                name,
                paint("skipped", Style::Yellow, use_color),
                reason
            );
            tally.skipped += 1;
        }
        TestOutcome::TimedOut { secs, steps } => {
            println!(
                "test {} ... {} after {}s (~{} steps)",
                name,
                paint("TIMEOUT", Style::Red, use_color),
                secs,
                format_step_count(*steps),
            );
            for line in TIMEOUT_HINT.lines() {
                println!("        {line}");
            }
            failures.push((
                name,
                format!(
                    "timed out after {secs}s (~{} steps) — evaluation did not terminate",
                    format_step_count(*steps)
                ),
            ));
            tally.timed_out += 1;
        }
        TestOutcome::BlackHole { cycle } => {
            println!(
                "test {} ... {} ({})",
                name,
                paint("BLACK HOLE", Style::Red, use_color),
                cycle.join(" → "),
            );
            for line in BLACK_HOLE_HINT.lines() {
                println!("        {line}");
            }
            failures.push((
                name,
                format!(
                    "black hole: {} — the definition re-enters itself while being \
                     forced and has no value (ADR 22.7.26a)",
                    cycle.join(" → ")
                ),
            ));
            tally.black_holed += 1;
        }
        TestOutcome::NeverCompared { reason } => {
            println!(
                "test {} ... {} ({})",
                name,
                paint("NEVER COMPARED", Style::Red, use_color),
                reason,
            );
            for line in NEVER_COMPARED_HINT.lines() {
                println!("        {line}");
            }
            failures.push((name, reason.clone()));
            tally.never_compared += 1;
        }
        TestOutcome::AssertedNothing | TestOutcome::DidNotFinish { .. } => {
            report_vacuity(name, outcome, tally, failures, use_color);
        }
        TestOutcome::ExpectedFailure { owner, reported } => {
            println!(
                "test {} ... {} ({}, owned by ADR {})",
                name,
                paint("EXPECTED FAILURE", Style::Yellow, use_color),
                reported.label(),
                owner,
            );
            tally.expected_failures += 1;
        }
    }
}

/// Print and tally the two ADR 6.8.26b vacuity outcomes.
///
/// Split out of `report_outcome` for the function-size limit, along the seam
/// that means something: these two share a verdict — "this test proved
/// nothing" — that none of the older outcomes carries.
fn report_vacuity<'a>(
    name: &'a str,
    outcome: &TestOutcome,
    tally: &mut Tally,
    failures: &mut Vec<(&'a str, String)>,
    use_color: bool,
) {
    match outcome {
        TestOutcome::AssertedNothing => {
            println!(
                "test {} ... {}",
                name,
                paint("ASSERTED NOTHING", Style::Red, use_color),
            );
            for line in ASSERTED_NOTHING_HINT.lines() {
                println!("        {line}");
            }
            failures.push((
                name,
                "the body reached a value without executing a single assertion \
                 (ADR 6.8.26b)"
                    .to_string(),
            ));
            tally.asserted_nothing += 1;
        }
        TestOutcome::DidNotFinish { assertions } => {
            println!(
                "test {} ... {} (after {assertions} assertion(s))",
                name,
                paint("DID NOT FINISH", Style::Red, use_color),
            );
            for line in DID_NOT_FINISH_HINT.lines() {
                println!("        {line}");
            }
            failures.push((
                name,
                format!(
                    "the body stopped making progress after {assertions} assertion(s) — \
                     it reached a residual, not Unit, so any later assertion never ran \
                     (ADR 6.8.26b)"
                ),
            ));
            tally.did_not_finish += 1;
        }
        _ => unreachable!("report_vacuity is only called for the two vacuity outcomes"),
    }
}

/// Render the failures detail block and the final summary line.
///
/// Pure — returns the text rather than printing it, so what the runner *says*
/// is assertable without capturing stdout. That matters more here than
/// elsewhere: this is the line a reader uses to decide whether a run proved
/// anything, and a `print_report` that silently printed nothing would look
/// identical to a clean run.
pub(super) fn format_report(
    tally: &Tally,
    failures: &[(&str, String)],
    elapsed: f64,
    use_color: bool,
) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    if !failures.is_empty() {
        out.push('\n');
        out.push_str(&paint("failures:", Style::Bold, use_color));
        out.push('\n');
        for (name, msg) in failures {
            let _ = writeln!(out, "  {name}:");
            for line in msg.lines() {
                let _ = writeln!(out, "    {line}");
            }
        }
    }

    let status = if tally.is_failing() {
        paint("FAILED", Style::BoldRed, use_color)
    } else {
        paint("ok", Style::BoldGreen, use_color)
    };
    // `write!` into a String is infallible, so the Result is discarded rather
    // than unwrapped — an unwrap here would be a panic path that cannot fire.
    let _ = write!(
        out,
        "\n{} {}. {} passed; {} failed; {} skipped{}{}{}{}{}{}; finished in {:.2}s",
        paint("result:", Style::Bold, use_color),
        status,
        tally.passed,
        tally.failed,
        tally.skipped,
        timed_out_clause(tally.timed_out),
        black_holed_clause(tally.black_holed),
        never_compared_clause(tally.never_compared),
        asserted_nothing_clause(tally.asserted_nothing),
        did_not_finish_clause(tally.did_not_finish),
        expected_failure_clause(tally.expected_failures),
        elapsed,
    );
    out
}

// No `print_report` wrapper: once the rendering is pure, a one-line
// `println!(format_report(..))` shim is a function whose only effect is stdout
// and which therefore no test can assert on — a permanent mutation survivor
// that documents nothing. `run_and_report` prints the rendered string directly.
