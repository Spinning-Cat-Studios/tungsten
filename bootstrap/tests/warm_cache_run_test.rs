//! Regression tests for ADR 4.7.26c — `run`/`test` warm-cache failure.
//!
//! Before the fix, `run`/`test` consumed the signature-only elaboration cache
//! (ADR 10.5.26n), which stores no `CoreDef` bodies. On a warm hit `output.defs`
//! was empty, so `run` couldn't find `main` (spurious `E0030` at the EOF span)
//! and `test` reported "no tests found". These tests drive the real library
//! entry point (`run_file_with_options`, evaluator path — no codegen) twice
//! against a shared isolated cache dir, asserting the warm run succeeds.
//!
//! The signature cache lives in `<file-parent>/.tungsten`, so each test writes
//! its fixture into a fresh `tempdir` to isolate the cache and let the second
//! (warm) call reuse what the first (cold) call wrote.

use std::path::{Path, PathBuf};

use tungsten_bootstrap::driver::{self, Mode, PipelineOpts, PipelineResult};
use tungsten_core::eval::term_to_nat;

/// `main` returns a value distinct from `helper`'s so a run that evaluates the
/// *wrong* def is caught by value, not just by variant: `evaluate_main` must
/// select `main` (→ 7), not the first non-main def (`helper` → 42). Without
/// the distinct values, flipping `find(name == "main")` to `!=` still yields
/// `Evaluated`, and the mutation gate had no in-process witness (ADR 22.7.26a
/// check-adr close-out).
const MAIN_VALUE: usize = 7;
const HELPER_VALUE: usize = 42;
const FIXTURE: &str = "\
fn main() -> Nat { 7 }
fn helper() -> Nat { 42 }
fn test_trivial() -> () { }
";

/// Extract the evaluated `Nat` from a `Run`-mode result, or panic with the
/// actual variant. Peano `Succ` chains and `NatLit` both decode via
/// `term_to_nat` (evaluator canonicalizes small Nats to `Succ` form).
fn evaluated_nat(result: PipelineResult) -> usize {
    match result {
        PipelineResult::Evaluated { value, .. } => {
            term_to_nat(&value).expect("main must evaluate to a Nat")
        }
        other => panic!("expected Evaluated, got {other:?}"),
    }
}

fn opts(mode: Mode) -> PipelineOpts {
    PipelineOpts {
        mode,
        verbose: false,
        dump_types: false,
    }
}

/// Write the fixture into a fresh tempdir; return (dir, canonical file path).
/// The `tempfile::TempDir` guard must stay alive for the cache dir to persist.
fn fixture_in_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prog.tg");
    std::fs::write(&path, FIXTURE).unwrap();
    (dir, path)
}

/// Run the pipeline with the cache ENABLED (the whole point — warm reuse).
fn run(path: &Path, mode: Mode) -> PipelineResult {
    driver::run_file_with_options(path, &opts(mode), /* no_cache */ false, 0)
        .expect("pipeline should not error")
}

#[test]
fn warm_run_finds_main() {
    let (_dir, path) = fixture_in_tempdir();

    // Assert the *value*, not just the variant: evaluating `helper` (42) would
    // also be `Evaluated`, so only the value proves `main` (7) was selected.
    assert_ne!(MAIN_VALUE, HELPER_VALUE, "fixture values must be distinct");

    // Cold: populates the signature cache.
    assert_eq!(
        evaluated_nat(run(&path, Mode::Run)),
        MAIN_VALUE,
        "cold run must evaluate main, not another def"
    );
    // Warm: pre-fix this returned Failed (empty defs → no `main` → E0030).
    assert_eq!(
        evaluated_nat(run(&path, Mode::Run)),
        MAIN_VALUE,
        "warm run must still evaluate main (ADR 4.7.26c)"
    );
}

#[test]
fn warm_run_finds_main_via_noncanonical_spelling() {
    let (_dir, path) = fixture_in_tempdir();
    // `<dir>/./prog.tg` — a non-canonical spelling of the same file. Proves the
    // failure was never about path spelling (3.7.26f's residual concern).
    let dotted = path.parent().unwrap().join(".").join("prog.tg");

    assert_eq!(evaluated_nat(run(&dotted, Mode::Run)), MAIN_VALUE);
    assert_eq!(
        evaluated_nat(run(&dotted, Mode::Run)),
        MAIN_VALUE,
        "warm run via non-canonical spelling must still evaluate main"
    );
}

#[test]
fn warm_test_finds_tests() {
    let (_dir, path) = fixture_in_tempdir();

    for label in ["cold", "warm"] {
        match run(&path, Mode::Test) {
            PipelineResult::Tested { defs, .. } => assert!(
                defs.iter().any(|d| d.name == "test_trivial"),
                "{label} test run must retain test_trivial in defs (ADR 4.7.26c)"
            ),
            other => panic!("{label} test run returned {other:?}, expected Tested"),
        }
    }
}

#[test]
fn check_then_run_cross_mode_gotcha() {
    let (_dir, path) = fixture_in_tempdir();
    // A `check` writes the bodyless signature entry; the later `run` must not
    // trip over it. This is the documented CLAUDE.md "no tests found" gotcha's
    // run-mode sibling.
    assert!(matches!(
        run(&path, Mode::Check),
        PipelineResult::Checked { .. }
    ));
    assert_eq!(
        evaluated_nat(run(&path, Mode::Run)),
        MAIN_VALUE,
        "run after a check must still find main"
    );
}

#[test]
fn check_then_test_cross_mode_gotcha() {
    let (_dir, path) = fixture_in_tempdir();
    assert!(matches!(
        run(&path, Mode::Check),
        PipelineResult::Checked { .. }
    ));
    match run(&path, Mode::Test) {
        PipelineResult::Tested { defs, .. } => assert!(
            defs.iter().any(|d| d.name == "test_trivial"),
            "test after a check must still discover test_trivial"
        ),
        other => panic!("test after check returned {other:?}, expected Tested"),
    }
}

#[test]
fn warm_check_still_succeeds() {
    let (_dir, path) = fixture_in_tempdir();
    // The fix must not disable check-mode caching: check keeps consuming the
    // signature cache. (The precise "read is taken" assertion is the unit test
    // `check_mode_consumes_signature_cache`; here we guard the end-to-end
    // behaviour — a warm check still type-checks with a stable def count.)
    let cold = run(&path, Mode::Check);
    let warm = run(&path, Mode::Check);
    match (cold, warm) {
        (
            PipelineResult::Checked { num_defs: a, .. },
            PipelineResult::Checked { num_defs: b, .. },
        ) => assert_eq!(a, b, "warm check def count must match cold"),
        other => panic!("expected two Checked results, got {other:?}"),
    }
}
