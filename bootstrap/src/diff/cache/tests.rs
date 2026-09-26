//! Unit tests for `tungsten diff cache` (ADR 4.7.26d §5): the cold-vs-warm
//! comparator, and one test per outcome class via the test-only overrides
//! (no compiler needed). Mirrors `diff exec`'s classifier tests.

use super::comparable;
use super::*;

fn rec(status: ExecStatus, stdout: &str) -> ExecRecord {
    ExecRecord {
        status,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

// ── Comparator ──────────────────────────────────────────────────────────

#[test]
fn equal_stdout_is_parity() {
    let outcome = classify(&rec(ExecStatus::Ok, "0\n"), &rec(ExecStatus::Ok, "0\n"));
    assert_eq!(outcome, Outcome::Parity);
    assert_eq!(outcome.exit_code(), 0);
}

#[test]
fn trailing_newlines_are_normalized() {
    assert_eq!(
        classify(&rec(ExecStatus::Ok, "0"), &rec(ExecStatus::Ok, "0\n")),
        Outcome::Parity
    );
}

#[test]
fn warm_diverges_is_divergence() {
    // The 4.7.26c shape: cold prints main's value, warm reads a bodyless
    // signature entry and reports "no tests found" / a spurious E0030.
    let outcome = classify(
        &rec(ExecStatus::Ok, "42\n"),
        &rec(ExecStatus::RuntimeError, "no tests found\n"),
    );
    assert_eq!(outcome, Outcome::Divergence);
    assert_eq!(outcome.exit_code(), 1);
}

#[test]
fn one_sided_runtime_error_is_divergence() {
    assert_eq!(
        classify(&rec(ExecStatus::Ok, ""), &rec(ExecStatus::RuntimeError, "")),
        Outcome::Divergence
    );
    assert_eq!(
        classify(&rec(ExecStatus::RuntimeError, ""), &rec(ExecStatus::Ok, "")),
        Outcome::Divergence
    );
}

#[test]
fn identical_runtime_error_is_parity() {
    // Both sides erroring the same way is agreement — stdout decides.
    assert_eq!(
        classify(
            &rec(ExecStatus::RuntimeError, "boom\n"),
            &rec(ExecStatus::RuntimeError, "boom\n"),
        ),
        Outcome::Parity
    );
}

#[test]
fn timeout_dominates() {
    assert_eq!(
        classify(&rec(ExecStatus::Timeout, ""), &rec(ExecStatus::Ok, "0")),
        Outcome::Timeout
    );
    assert_eq!(
        classify(&rec(ExecStatus::Ok, "0"), &rec(ExecStatus::Timeout, "")),
        Outcome::Timeout
    );
}

#[test]
fn exit_codes_are_the_section_2_2_subset() {
    assert_eq!(Outcome::Parity.exit_code(), 0);
    assert_eq!(Outcome::Divergence.exit_code(), 1);
    assert_eq!(Outcome::CompileError.exit_code(), 3);
    assert_eq!(Outcome::Timeout.exit_code(), 5);
}

#[test]
fn report_labels_both_sides_and_verdict() {
    let report = render_report(
        Outcome::Divergence,
        &rec(ExecStatus::Ok, "42\n"),
        &rec(ExecStatus::RuntimeError, "no tests found\n"),
        Duration::from_secs(60),
    );
    assert!(report.contains("cold (fresh cache)"), "{report}");
    assert!(report.contains("warm (reused cache)"), "{report}");
    assert!(report.contains("cache poisoning"), "{report}");
    assert!(report.contains("42"), "{report}");
}

// ── Outcome classes end-to-end via the testability hook ─────────────────

fn argv(parts: &[&str]) -> Option<Vec<String>> {
    Some(parts.iter().map(ToString::to_string).collect())
}

/// Any existing file passes the input check; cold-overridden runs never read it
/// and skip the elaboration probe.
fn dummy_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("dummy.tg");
    std::fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    (dir, path)
}

fn run(overrides: &CacheOverrides, timeout: Duration) -> Outcome {
    let (_dir, file) = dummy_file();
    run_diff_cache(&file, "run", timeout, overrides).unwrap()
}

const T: Duration = Duration::from_secs(30);

#[test]
fn override_parity_exits_0() {
    let overrides = CacheOverrides {
        cold: argv(&["echo", "0"]),
        warm: argv(&["echo", "0"]),
    };
    assert_eq!(run(&overrides, T), Outcome::Parity);
}

#[test]
fn override_divergence_exits_1() {
    // cold ok, warm fails — the cache-poisoning signature.
    let overrides = CacheOverrides {
        cold: argv(&["echo", "42"]),
        warm: argv(&["sh", "-c", "echo 'no tests found'; exit 1"]),
    };
    assert_eq!(run(&overrides, T), Outcome::Divergence);
}

#[test]
fn override_timeout_exits_5() {
    let overrides = CacheOverrides {
        cold: argv(&["sleep", "5"]),
        warm: argv(&["echo", "0"]),
    };
    assert_eq!(
        run(&overrides, Duration::from_millis(100)),
        Outcome::Timeout
    );
}

#[test]
fn missing_file_is_compile_error_class() {
    let overrides = CacheOverrides::default();
    let err = run_diff_cache(Path::new("/nonexistent/nope.tg"), "run", T, &overrides);
    assert!(err.is_err(), "missing file must not run either side");
}

// ── Duration elision (the `--mode test --gate` flake) ───────────────────────

/// The warm run is faster by construction, so the test runner's wall-clock
/// summary differs between the two sides. That is observation noise, and
/// comparing it verbatim made `--mode test --gate` flake 3 runs in 5.
#[test]
fn a_differing_duration_is_not_a_divergence() {
    let cold = "result: ok. 3 passed; 0 failed; 0 skipped; finished in 0.01s";
    let warm = "result: ok. 3 passed; 0 failed; 0 skipped; finished in 0.00s";
    assert_eq!(comparable(cold), comparable(warm));
}

/// …but the counts on that same line still decide parity. Cache poisoning
/// changes those (4.7.26c turned them into "no tests found"), so eliding the
/// duration must not elide the signal beside it.
#[test]
fn differing_test_counts_are_still_a_divergence() {
    let cold = "result: ok. 3 passed; 0 failed; 0 skipped; finished in 0.01s";
    let warm = "result: ok. 0 passed; 0 failed; 0 skipped; finished in 0.01s";
    assert_ne!(comparable(cold), comparable(warm));
}

/// The 4.7.26c shape itself: a warm run that finds no tests at all.
#[test]
fn the_no_tests_found_regression_is_a_divergence() {
    let cold = "running 3 tests\nresult: ok. 3 passed; finished in 0.01s";
    let warm = "running 0 tests\nresult: ok. 0 passed; finished in 0.00s";
    assert_ne!(comparable(cold), comparable(warm));
}

/// Output with no duration marker passes through unchanged apart from the
/// pre-existing trailing-newline trim — `--mode run` must be unaffected.
#[test]
fn output_without_a_duration_is_untouched() {
    assert_eq!(
        comparable("line one\npartial done\n\n0\n"),
        "line one\npartial done\n\n0"
    );
}

/// Multiple duration markers are all elided (a multi-suite run).
#[test]
fn every_duration_marker_is_elided() {
    let cold = "a finished in 1.5s\nb finished in 2.25s";
    let warm = "a finished in 0.5s\nb finished in 0.25s";
    assert_eq!(comparable(cold), comparable(warm));
    assert!(comparable(cold).contains("<elided>"));
}

/// A trailing marker with no digits after it does not panic or eat the rest.
#[test]
fn a_truncated_duration_marker_is_safe() {
    assert_eq!(comparable("finished in "), "finished in <elided>");
}

/// The exact elided form, pinned. The cold-vs-warm equality tests above pass
/// for any *consistent* mangling, so they cannot tell a correct elision from
/// one that drops the trailing unit or truncates the tail — only asserting the
/// output shape can.
#[test]
fn the_duration_and_its_unit_are_both_consumed() {
    assert_eq!(comparable("finished in 0.01s"), "finished in <elided>");
    assert_eq!(
        comparable("result: ok. 3 passed; finished in 12.5s"),
        "result: ok. 3 passed; finished in <elided>"
    );
}

/// Text after the duration survives — the elision consumes the number and its
/// unit, not the remainder of the line.
#[test]
fn text_after_the_duration_survives() {
    assert_eq!(
        comparable("finished in 0.01s\nnext line"),
        "finished in <elided>\nnext line"
    );
}
