//! Tests for the per-test watchdog (ADR 21.7.26f / D1).
//!
//! Kept in a sibling file rather than inside `run.rs` so that file stays under
//! the 400-LOC health limit. Tests: <this file>.

use super::super::summary::{black_holed_clause, format_step_count, timed_out_clause};
use super::{report_outcome, run_and_report, RunOptions, Tally};
use crate::cli::{Cli, Commands};
use crate::test_runner::{TestFunction, TestOutcome};
use clap::Parser;
use std::process::ExitCode;
use tungsten_bootstrap::comparator::ComparatorTypes;
use tungsten_bootstrap::elaborate::CoreDef;
use tungsten_bootstrap::span::Span;
use tungsten_core::terms::SpannedTerm;
use tungsten_core::{Term, Type};

fn make_def(name: &str, term: Term) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty: Type::Unit,
        term: SpannedTerm::generated(term),
        span: Span::new(0, 0),
    }
}

/// A body that executes exactly one real assertion and reduces to `Unit`.
///
/// The watchdog fixtures used a bare `Term::Unit` until ADR 6.8.26b, which was
/// fine when "finished" was the only thing the runner checked. It now also
/// checks that a test *asserted*, and a `Unit` body asserts nothing — so these
/// fixtures were being reported `ASSERTED NOTHING` and failing the very runs
/// they exist to prove succeed. Asserting `1 == 1` keeps them exercising the
/// watchdog while satisfying the new gate honestly, rather than exempting them.
fn passing_body() -> Term {
    Term::ExternCall(
        "__c_tg_assert_eq_nat".to_string(),
        vec![Term::nat(1), Term::nat(1)],
    )
}

/// `fix f. f` — steps forever at constant term size, which is what a
/// non-terminating *stepping* test body reduces to.
fn diverging_body() -> Term {
    Term::fix("f", Type::Unit, Term::var("f"))
}

// ── step-count forensics rendering ──────────────────────────────────────────

#[test]
fn step_counts_below_ten_thousand_render_exactly() {
    // Small counts are more informative unrounded — "3 steps" says the body
    // barely ran, which "0.0K" would hide.
    assert_eq!(format_step_count(0), "0");
    assert_eq!(format_step_count(3), "3");
    assert_eq!(format_step_count(9_999), "9999");
}

#[test]
fn step_counts_in_the_thousands_render_with_a_k_suffix() {
    assert_eq!(format_step_count(10_000), "10.0K");
    assert_eq!(format_step_count(410_000), "410.0K");
    // Truncated, not rounded: just under a million must not read as "1000.0K".
    assert_eq!(format_step_count(999_999), "999.9K");
}

#[test]
fn step_counts_in_the_millions_render_with_an_m_suffix() {
    // The ADR's worked example: 4.1 million steps reads as "4.1M".
    assert_eq!(format_step_count(1_000_000), "1.0M");
    assert_eq!(format_step_count(4_100_000), "4.1M");
    assert_eq!(format_step_count(41_000_000), "41.0M");
}

// ── summary-line clause ─────────────────────────────────────────────────────

#[test]
fn no_timeouts_produce_an_empty_summary_clause() {
    // The zero-noise guarantee: an untripped run must print the exact
    // pre-watchdog summary line that the golden .expected files record.
    assert_eq!(timed_out_clause(0), "");
}

#[test]
fn timeouts_are_named_in_the_summary_clause() {
    assert_eq!(timed_out_clause(1), "; 1 timed out");
    assert_eq!(timed_out_clause(3), "; 3 timed out");
}

// ── exit-code gating ────────────────────────────────────────────────────────

#[test]
fn a_clean_tally_is_not_failing() {
    let tally = Tally {
        passed: 4,
        skipped: 2,
        ..Default::default()
    };
    assert!(!tally.is_failing());
}

#[test]
fn a_timeout_alone_makes_the_run_fail() {
    // The whole point of D1: a hang must become a nonzero exit, not a green run.
    let tally = Tally {
        passed: 3,
        timed_out: 1,
        ..Default::default()
    };
    assert!(tally.is_failing());
}

#[test]
fn an_assertion_failure_alone_makes_the_run_fail() {
    let tally = Tally {
        failed: 1,
        ..Default::default()
    };
    assert!(tally.is_failing());
}

// ── outcome reporting ───────────────────────────────────────────────────────

#[test]
fn a_timeout_outcome_tallies_separately_and_records_a_failure_detail() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let outcome = TestOutcome::TimedOut {
        secs: 60,
        steps: 4_100_000,
    };

    report_outcome("test_spins", &outcome, &mut tally, &mut failures, false);

    assert_eq!(tally.timed_out, 1);
    assert_eq!(
        tally.failed, 0,
        "timeouts tally apart from assertion failures"
    );
    assert_eq!(tally.passed, 0);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "test_spins");
    assert!(
        failures[0].1.contains("timed out after 60s"),
        "detail must name the bound: {}",
        failures[0].1
    );
    assert!(
        failures[0].1.contains("4.1M"),
        "detail must carry the step forensics: {}",
        failures[0].1
    );
}

#[test]
fn a_passing_outcome_leaves_the_timeout_tally_untouched() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    report_outcome(
        "test_ok",
        &TestOutcome::Passed,
        &mut tally,
        &mut failures,
        false,
    );
    assert_eq!(tally.passed, 1);
    assert_eq!(tally.timed_out, 0);
    assert!(failures.is_empty());
}

// ── end-to-end runner behaviour ─────────────────────────────────────────────
//
// Driven in-process rather than by spawning `tungsten test`: a body that never
// terminates is exactly the thing that must not be left running, and the
// 21.7.26c session's unkillable `UE` processes came from doing this out-of-process.

fn run_with_watchdog(defs: &[CoreDef], watchdog_secs: u64) -> ExitCode {
    let tests: Vec<TestFunction> = defs
        .iter()
        .filter(|d| d.name.starts_with("test_"))
        .map(|d| TestFunction {
            name: d.name.clone(),
        })
        .collect();
    let opts = RunOptions {
        check_only: false,
        watchdog_secs,
        use_color: false,
        assertion_census: false,
        expected_failures: std::collections::BTreeMap::new(),
    };
    run_and_report(&tests, defs, &ComparatorTypes::default(), &opts)
}

#[test]
fn a_diverging_test_body_trips_the_watchdog_and_fails_the_run() {
    let defs = vec![make_def("test_spins", diverging_body())];
    let start = std::time::Instant::now();

    let exit = run_with_watchdog(&defs, 1);

    assert_eq!(
        exit,
        ExitCode::FAILURE,
        "a test that never terminates must not exit green"
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(30),
        "the watchdog must bound the run, not merely observe it"
    );
}

#[test]
fn tests_after_a_timeout_still_execute() {
    // The runner continues past a trip — one hanging test must not cost the
    // whole suite's signal.
    let defs = vec![
        make_def("test_spins", diverging_body()),
        // Asserts, so the run's failure is attributable to the timeout alone
        // and not to `test_after` itself tripping the 6.8.26b gate.
        make_def("test_after", passing_body()),
    ];

    let exit = run_with_watchdog(&defs, 1);

    // `test_after` running is what makes the run reach its (failing) report;
    // a runner that aborted on the first trip could not distinguish the two.
    assert_eq!(exit, ExitCode::FAILURE);
}

#[test]
fn a_terminating_suite_is_unaffected_by_the_watchdog() {
    let defs = vec![make_def("test_ok", passing_body())];
    assert_eq!(run_with_watchdog(&defs, 60), ExitCode::SUCCESS);
}

/// 14.9.26c AC 2: `assert_eq_int` is a counted assertion, so a test made of
/// nothing else is `ok` rather than `ASSERTED NOTHING`; and 14.9.26c AC 4: a
/// body that traps is a FAILED test, never a green one.
#[test]
fn int_assertions_count_and_int_traps_fail_the_run() {
    use tungsten_core::terms::IntBinOp;
    let asserts_int = Term::ExternCall(
        "__c_tg_assert_eq_int".to_string(),
        vec![Term::int_lit(-1), Term::int_lit(-1)],
    );
    assert_eq!(
        run_with_watchdog(&[make_def("test_int", asserts_int)], 60),
        ExitCode::SUCCESS
    );

    let traps = Term::ExternCall(
        "__c_tg_assert_eq_int".to_string(),
        vec![
            Term::int_bin(IntBinOp::Add, Term::int_lit(i64::MAX), Term::int_lit(1)),
            Term::int_lit(0),
        ],
    );
    assert_eq!(
        run_with_watchdog(&[make_def("test_trap", traps)], 60),
        ExitCode::FAILURE
    );
}

#[test]
fn watchdog_zero_takes_the_unbounded_path() {
    // `--watchdog 0` restores the pre-21.7.26f behaviour. Exercised on a
    // terminating body — the whole point of the sentinel is that nothing
    // bounds evaluation, so a diverging body here would never return.
    let defs = vec![make_def("test_ok", passing_body())];
    assert_eq!(run_with_watchdog(&defs, 0), ExitCode::SUCCESS);
}

// ── black-hole reporting (ADR 22.7.26a) ─────────────────────────────────────
//
// The §1.3 silent-green trap: a black-holed test maps interiorly onto a stuck
// term, and a stuck term is a *passing* outcome — so every assertion here
// exists to prove the black hole is a named, counted, run-failing outcome.

/// The reproducer shape as defs: a 0-arg global whose body is itself.
fn self_referential_defs() -> Vec<CoreDef> {
    vec![
        make_def("loop_forever", Term::Global("loop_forever".into())),
        make_def("test_black_hole", Term::Global("loop_forever".into())),
    ]
}

#[test]
fn a_black_hole_outcome_tallies_separately_and_records_a_failure_detail() {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    let outcome = TestOutcome::BlackHole {
        cycle: vec!["loop_forever".to_string(), "loop_forever".to_string()],
    };

    report_outcome(
        "test_black_hole",
        &outcome,
        &mut tally,
        &mut failures,
        false,
    );

    assert_eq!(tally.black_holed, 1);
    assert_eq!(
        tally.failed, 0,
        "black holes tally apart from assertion failures"
    );
    assert_eq!(tally.timed_out, 0, "black holes tally apart from timeouts");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "test_black_hole");
    assert!(
        failures[0].1.contains("loop_forever → loop_forever"),
        "detail must name the cycle: {}",
        failures[0].1
    );
}

#[test]
fn a_black_hole_alone_makes_the_run_fail() {
    let tally = Tally {
        passed: 3,
        black_holed: 1,
        ..Default::default()
    };
    assert!(tally.is_failing());
}

#[test]
fn no_black_holes_produce_an_empty_summary_clause() {
    // The 21.7.26f summary-line constraint: an untripped run's summary stays
    // byte-identical, so the golden .expected files stand.
    assert_eq!(black_holed_clause(0), "");
}

#[test]
fn black_holes_are_named_in_the_summary_clause() {
    assert_eq!(black_holed_clause(1), "; 1 black-holed");
    assert_eq!(black_holed_clause(2), "; 2 black-holed");
}

#[test]
fn a_black_holed_test_fails_the_run_end_to_end() {
    // In-process end-to-end: pre-fix this overflowed the stack and aborted
    // the whole test binary; completing with FAILURE is itself the fix.
    let exit = run_with_watchdog(&self_referential_defs(), 60);
    assert_eq!(
        exit,
        ExitCode::FAILURE,
        "a black-holed test must never exit green (§1.3 silent-green trap)"
    );
}

#[test]
fn a_black_holed_test_fails_the_run_on_the_unbounded_path_too() {
    // `--watchdog 0` takes `eval_with_env`, not `eval_with_env_until`; the
    // black hole must be reported on that entry as well (D3).
    let exit = run_with_watchdog(&self_referential_defs(), 0);
    assert_eq!(exit, ExitCode::FAILURE);
}

#[test]
fn tests_after_a_black_hole_still_execute() {
    let mut defs = self_referential_defs();
    defs.push(make_def("test_after", Term::Unit));
    assert_eq!(run_with_watchdog(&defs, 60), ExitCode::FAILURE);
}

// ── flag surface ────────────────────────────────────────────────────────────

#[test]
fn watchdog_defaults_to_sixty_seconds() {
    let cli = Cli::try_parse_from(["tungsten", "test", "file.tg"]).unwrap();
    match cli.command {
        Some(Commands::Test { watchdog, .. }) => assert_eq!(watchdog, 60),
        other => panic!("expected Test command, got {:?}", other.map(|_| "other")),
    }
}

#[test]
fn watchdog_flag_overrides_the_default() {
    let cli = Cli::try_parse_from(["tungsten", "test", "file.tg", "--watchdog", "300"]).unwrap();
    match cli.command {
        Some(Commands::Test { watchdog, .. }) => assert_eq!(watchdog, 300),
        other => panic!("expected Test command, got {:?}", other.map(|_| "other")),
    }
}

#[test]
fn watchdog_zero_parses_as_the_disable_sentinel() {
    let cli = Cli::try_parse_from(["tungsten", "test", "file.tg", "--watchdog", "0"]).unwrap();
    match cli.command {
        Some(Commands::Test { watchdog, .. }) => assert_eq!(watchdog, 0),
        other => panic!("expected Test command, got {:?}", other.map(|_| "other")),
    }
}
