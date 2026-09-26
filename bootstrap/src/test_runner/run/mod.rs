//! Test execution + reporting for `tungsten test` (ADR 29.6.26f / T13).
//!
//! Each `test_*` body is evaluated through the bootstrap evaluator (which executes
//! the assertion FFIs, P6-runtime) and the runtime failure flag is consulted to
//! decide `Passed`/`Failed`. Previously every elaborated test was reported `ok`
//! unconditionally — a silent-failure trap where `assert_eq(1, 2)` passed.

use std::collections::{BTreeMap, HashMap};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tungsten_bootstrap::comparator::ComparatorTypes;
use tungsten_bootstrap::elaborate::CoreDef;
use tungsten_core::eval::{eval_with_env, eval_with_env_until, EvalStopped};
use tungsten_core::Term;

use super::summary::{apply_expected_failure, census_line, classify_finished_body};
use super::{TestFunction, TestOutcome};

mod report;
use report::{format_report, report_outcome, Tally};

/// Options controlling a whole `tungsten test` run.
pub(super) struct RunOptions {
    pub check_only: bool,
    /// Per-test wall-clock bound in seconds; `0` disables the watchdog.
    pub watchdog_secs: u64,
    pub use_color: bool,
    /// Print the per-test executed-assertion count (ADR 6.8.26b D4).
    pub assertion_census: bool,
    /// Tests the manifest permits to fail, mapped to the successor ADR that
    /// owns removing each entry (ADR 6.8.26c D6). Empty is the normal state.
    pub expected_failures: BTreeMap<String, String>,
}

/// Evaluate a single `test_*` body and report `Passed`/`Failed` from the runtime
/// failure flag. A 0-arg `test_*` stores its body directly (like `main`), so
/// evaluating the def's term runs the body — including any assertion FFIs.
///
/// Evaluation runs under a wall-clock watchdog (ADR 21.7.26f / D1) so a
/// non-terminating body becomes a named TIMEOUT failure rather than a silent
/// hang; `watchdog_secs == 0` restores the unbounded behaviour.
fn run_test_body(
    name: &str,
    defs: &[CoreDef],
    globals: &HashMap<String, Term>,
    comparator_types: &ComparatorTypes,
    watchdog_secs: u64,
) -> (TestOutcome, u64) {
    let Some(def) = defs.iter().find(|d| d.name == name) else {
        return (
            TestOutcome::Failed("test definition not found after discovery".to_string()),
            0,
        );
    };

    // Reset the per-test failure flag (null name is safe — the FFI null-checks it).
    tungsten_core::ffi::tg_test_begin(std::ptr::null(), 0);

    let env = tungsten_bootstrap::comparator::eval::eval_env(globals.clone(), comparator_types);
    let evaluated = if watchdog_secs == 0 {
        eval_with_env(&def.term.term, &env)
    } else {
        let deadline = Instant::now() + Duration::from_secs(watchdog_secs);
        eval_with_env_until(&def.term.term, &env, deadline)
    };
    let finished_value = match evaluated {
        Ok(value) => value,
        Err(EvalStopped::TimedOut { steps }) => {
            return (
                TestOutcome::TimedOut {
                    secs: watchdog_secs,
                    steps,
                },
                env.assertions_executed(),
            );
        }
        Err(EvalStopped::BlackHole { cycle }) => {
            return (TestOutcome::BlackHole { cycle }, env.assertions_executed());
        }
        // A comparison that never ran (ADR 1.8.26b D3). Reported before the
        // failure flag is consulted, because the flag is exactly what such a
        // test fails to set.
        Err(
            stopped @ (EvalStopped::Uncomparable(_)
            | EvalStopped::ComparisonNeverRan { .. }
            | EvalStopped::MalformedElimination { .. }),
        ) => {
            return (
                TestOutcome::NeverCompared {
                    reason: stopped.to_string(),
                },
                env.assertions_executed(),
            );
        }
        // The runner sets no step limit, so this stop cannot occur here; if
        // a limit is ever wired in, it must fail the test, never pass it.
        // An `Int` trap (ADR 14.9.26c) is a program error and fails the same
        // way — natively it would have aborted the harness.
        Err(stopped @ (EvalStopped::StepLimit { .. } | EvalStopped::IntTrap { .. })) => {
            return (
                TestOutcome::Failed(stopped.to_string()),
                env.assertions_executed(),
            );
        }
    };

    let assertions = env.assertions_executed();
    let outcome = classify_finished_body(
        tungsten_core::ffi::tg_test_check_failure() == 1,
        assertions,
        matches!(finished_value, Term::Unit),
    );
    (outcome, assertions)
}

/// Run all tests and print results; exit code is `FAILURE` iff any test failed
/// or tripped the watchdog.
pub(super) fn run_and_report(
    tests: &[TestFunction],
    defs: &[CoreDef],
    comparator_types: &ComparatorTypes,
    opts: &RunOptions,
) -> ExitCode {
    let use_color = opts.use_color;
    let total = tests.len();
    println!(
        "\nrunning {} test{}",
        total,
        if total == 1 { "" } else { "s" }
    );
    println!();

    // Globals for environment-based evaluation of each test body — the same map
    // `tungsten run` builds for `main`, carrying the lazy `__cmp<T>` comparator
    // synthesis callback so `compare`/`assert_*` resolve (ADR 29.6.26f §T11.2a).
    let globals: HashMap<String, Term> = defs
        .iter()
        .map(|d| (d.name.clone(), d.term.term.clone()))
        .collect();

    let mut tally = Tally::default();
    let mut failures: Vec<(&str, String)> = Vec::new();
    let start = Instant::now();

    for test in tests {
        let (outcome, assertions) = if opts.check_only {
            // check-only: `expect_type`/`expect_error` already ran during elaboration.
            (TestOutcome::Skipped("check-only".to_string()), 0)
        } else {
            run_test_body(
                &test.name,
                defs,
                &globals,
                comparator_types,
                opts.watchdog_secs,
            )
        };
        let outcome = apply_expected_failure(
            outcome,
            opts.expected_failures.get(&test.name).map(String::as_str),
        );
        report_outcome(&test.name, &outcome, &mut tally, &mut failures, use_color);
        if opts.assertion_census {
            println!("{}", census_line(assertions));
        }
    }

    println!(
        "{}",
        format_report(&tally, &failures, start.elapsed().as_secs_f64(), use_color)
    );

    if tally.is_failing() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
// Tests: tests_watchdog.rs
#[path = "tests_watchdog.rs"]
mod tests_watchdog;

#[cfg(test)]
// Tests: tests_never_compared.rs
#[path = "tests_never_compared.rs"]
mod tests_never_compared;

#[cfg(test)]
// Tests: tests_asserted_nothing.rs
#[path = "tests_asserted_nothing.rs"]
mod tests_asserted_nothing;

#[cfg(test)]
// Tests: tests_expected_failure.rs
#[path = "tests_expected_failure.rs"]
mod tests_expected_failure;
