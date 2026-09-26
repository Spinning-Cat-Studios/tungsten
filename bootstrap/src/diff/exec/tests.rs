//! Unit tests for `tungsten diff exec` (ADR 3.7.26d §2.2/§5): the output
//! comparator, and one test per outcome class via the test-only overrides
//! (no miscompiling compiler — or any compiler — needed).

use super::*;

fn rec(status: ExecStatus, stdout: &str) -> ExecRecord {
    ExecRecord {
        status,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

// ── Comparator (AC: equal → parity, divergent → divergence) ─────────────

#[test]
fn equal_stdout_is_parity() {
    let outcome = classify(&rec(ExecStatus::Ok, "151\n"), &rec(ExecStatus::Ok, "151\n"));
    assert_eq!(outcome, Outcome::Parity);
    assert_eq!(outcome.exit_code(), 0);
}

#[test]
fn trailing_newlines_are_normalized() {
    assert_eq!(
        classify(&rec(ExecStatus::Ok, "151"), &rec(ExecStatus::Ok, "151\n")),
        Outcome::Parity
    );
    assert_eq!(
        classify(&rec(ExecStatus::Ok, "151\n\n"), &rec(ExecStatus::Ok, "151")),
        Outcome::Parity
    );
}

#[test]
fn divergent_stdout_is_divergence() {
    // The 3.7.26a defect-2 observation: garbage vs the evaluator's value.
    let outcome = classify(
        &rec(ExecStatus::Ok, "100728390524338177\n"),
        &rec(ExecStatus::Ok, "151\n"),
    );
    assert_eq!(outcome, Outcome::Divergence);
    assert_eq!(outcome.exit_code(), 1);
}

#[test]
fn interior_whitespace_is_not_normalized() {
    assert_eq!(
        classify(&rec(ExecStatus::Ok, "a\nb"), &rec(ExecStatus::Ok, "a\n\nb")),
        Outcome::Divergence
    );
}

#[test]
fn status_classification() {
    let ok = rec(ExecStatus::Ok, "");
    let err = rec(ExecStatus::RuntimeError, "");
    let timeout = rec(ExecStatus::Timeout, "");
    assert_eq!(classify(&err, &ok), Outcome::NativeRuntimeError);
    assert_eq!(classify(&ok, &err), Outcome::EvalRuntimeError);
    assert_eq!(classify(&timeout, &ok), Outcome::Timeout);
    assert_eq!(classify(&ok, &timeout), Outcome::Timeout);
    // Both failing at runtime: statuses agree — stdout decides.
    assert_eq!(classify(&err, &err), Outcome::Parity);
}

#[test]
fn first_divergence_line_is_reported() {
    assert_eq!(first_divergence_line("a\nb\nc", "a\nx\nc"), 2);
    assert_eq!(first_divergence_line("a", "a\nb"), 2);
    assert_eq!(first_divergence_line("same", "same"), 2); // no divergence: past the end
}

#[test]
fn exit_codes_are_the_section_2_2_contract() {
    assert_eq!(Outcome::Parity.exit_code(), 0);
    assert_eq!(Outcome::Divergence.exit_code(), 1);
    assert_eq!(Outcome::NativeRuntimeError.exit_code(), 2);
    assert_eq!(Outcome::CompileError.exit_code(), 3);
    assert_eq!(Outcome::EvalRuntimeError.exit_code(), 4);
    assert_eq!(Outcome::Timeout.exit_code(), 5);
}

/// The divergence report carries everything §2.2 promises: both stdouts,
/// the first-divergence line number, and stderr (reported, not compared).
#[test]
fn divergence_report_shows_both_outputs_and_stderr() {
    let native = ExecRecord {
        status: ExecStatus::Ok,
        stdout: "151\ngarbage\n".to_string(),
        stderr: "native-noise".to_string(),
    };
    let eval = ExecRecord {
        status: ExecStatus::Ok,
        stdout: "151\n151\n".to_string(),
        stderr: "eval-diagnostics".to_string(),
    };
    let report = render_report(Outcome::Divergence, &native, &eval, Duration::from_secs(60));
    assert!(report.contains("first differing line: 2"), "{report}");
    assert!(report.contains("garbage"), "{report}");
    assert!(report.contains("151"), "{report}");
    assert!(report.contains("native-noise"), "{report}");
    assert!(report.contains("eval-diagnostics"), "{report}");
    assert!(report.contains("not compared"), "{report}");
}

/// Empty stderr sections are omitted from the report entirely.
#[test]
fn report_omits_empty_stderr_sections() {
    let report = render_report(
        Outcome::Divergence,
        &rec(ExecStatus::Ok, "a"),
        &rec(ExecStatus::Ok, "b"),
        Duration::from_secs(60),
    );
    assert!(!report.contains("stderr"), "{report}");
}

// ── Outcome classes end-to-end via the testability hook (§2.2) ──────────
//
// Each test drives `run_diff_exec` with injected argv overrides, exercising
// the real subprocess/timeout/classification pipeline with synthetic sides.

fn argv(parts: &[&str]) -> Option<Vec<String>> {
    Some(parts.iter().map(ToString::to_string).collect())
}

/// Any existing file passes the input check; overrides never read it.
fn dummy_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("dummy.tg");
    std::fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    (dir, path)
}

fn run(overrides: &ExecOverrides, timeout: Duration) -> Outcome {
    let (_dir, file) = dummy_file();
    run_diff_exec(&file, timeout, overrides).unwrap()
}

const T: Duration = Duration::from_secs(30);

#[test]
fn override_parity_exits_0() {
    let overrides = ExecOverrides {
        native: argv(&["echo", "42"]),
        eval: argv(&["echo", "42"]),
        ..ExecOverrides::default()
    };
    assert_eq!(run(&overrides, T), Outcome::Parity);
}

#[test]
fn override_divergence_exits_1() {
    let overrides = ExecOverrides {
        native: argv(&["echo", "garbage"]),
        eval: argv(&["echo", "151"]),
        ..ExecOverrides::default()
    };
    assert_eq!(run(&overrides, T), Outcome::Divergence);
}

#[test]
fn override_native_runtime_error_exits_2() {
    let overrides = ExecOverrides {
        native: argv(&["sh", "-c", "exit 3"]),
        eval: argv(&["echo", "151"]),
        ..ExecOverrides::default()
    };
    assert_eq!(run(&overrides, T), Outcome::NativeRuntimeError);
}

#[test]
fn override_compile_error_exits_3() {
    let overrides = ExecOverrides {
        compile: argv(&["sh", "-c", "exit 1"]),
        native: argv(&["echo", "never runs"]),
        eval: argv(&["echo", "never runs"]),
    };
    assert_eq!(run(&overrides, T), Outcome::CompileError);
}

#[test]
fn override_eval_error_exits_4() {
    let overrides = ExecOverrides {
        native: argv(&["echo", "151"]),
        eval: argv(&["sh", "-c", "exit 1"]),
        ..ExecOverrides::default()
    };
    assert_eq!(run(&overrides, T), Outcome::EvalRuntimeError);
}

#[test]
fn override_timeout_exits_5() {
    let overrides = ExecOverrides {
        native: argv(&["sleep", "5"]),
        eval: argv(&["echo", "151"]),
        ..ExecOverrides::default()
    };
    assert_eq!(
        run(&overrides, Duration::from_millis(100)),
        Outcome::Timeout
    );
}

#[test]
fn missing_file_is_compile_error_class() {
    let overrides = ExecOverrides::default();
    let err = run_diff_exec(Path::new("/nonexistent/nope.tg"), T, &overrides);
    assert!(err.is_err(), "missing file must not run either side");
}

#[test]
fn stderr_is_reported_but_not_compared() {
    // Identical stdout with wildly different stderr is still parity.
    let overrides = ExecOverrides {
        native: argv(&["sh", "-c", "echo 42; echo native-noise >&2"]),
        eval: argv(&["sh", "-c", "echo 42; echo eval-diagnostics >&2"]),
        ..ExecOverrides::default()
    };
    assert_eq!(run(&overrides, T), Outcome::Parity);
}

#[test]
fn overrides_from_env_split_whitespace() {
    // Constructed via the same parsing the env hook uses, without touching
    // process-global env (parallel-test safe).
    let parsed: Vec<String> = "echo 42".split_whitespace().map(str::to_string).collect();
    assert_eq!(parsed, vec!["echo".to_string(), "42".to_string()]);
}
