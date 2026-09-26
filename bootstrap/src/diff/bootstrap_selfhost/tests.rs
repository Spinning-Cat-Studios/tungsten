//! Tests for `tungsten diff bootstrap-selfhost-check` (ADRs 20.5.26a, 24.7.26e).

use super::*;

#[test]
fn nonexistent_file_fails_gracefully() {
    let result = cmd_diff_bootstrap_selfhost_check(
        Path::new("/nonexistent/test.tg"),
        Path::new("./tungsten1"),
        false,
    );
    assert_ne!(result, ExitCode::SUCCESS);
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn both_clean_is_agreement() {
    assert_eq!(report_divergence(&[], &[], false), ExitCode::SUCCESS);
}

#[test]
fn identical_error_sets_are_agreement() {
    let errs = strings(&["error[E0001]: boom"]);
    assert_eq!(report_divergence(&errs, &errs, false), ExitCode::SUCCESS);
}

#[test]
fn any_divergence_fails() {
    // Both directions must fail: a self-host-only error is the regression
    // signal, and a bootstrap-only error is still a disagreement.
    let bootstrap = strings(&["error[E0001]: boom"]);
    let selfhost = strings(&["error[E0002]: bang"]);
    assert_ne!(
        report_divergence(&bootstrap, &selfhost, false),
        ExitCode::SUCCESS
    );
    assert_ne!(report_divergence(&bootstrap, &[], false), ExitCode::SUCCESS);
    assert_ne!(report_divergence(&[], &selfhost, false), ExitCode::SUCCESS);
}

#[test]
fn same_errors_in_a_different_order_count_as_divergence() {
    // Comparison is order-sensitive by design (it is a `==` on the captured
    // line sequence), but neither side has an error the other lacks — so
    // the set difference is empty and only the ordering differs.
    let bootstrap = strings(&["error: a", "error: b"]);
    let selfhost = strings(&["error: b", "error: a"]);
    assert_ne!(
        report_divergence(&bootstrap, &selfhost, false),
        ExitCode::SUCCESS
    );
    assert!(errors_missing_from(&selfhost, &bootstrap).is_empty());
}

#[test]
fn preflight_names_which_input_is_missing() {
    // The two guards are distinguishable only by WHICH one fires — both
    // produce the same exit code, so a code-only assertion cannot tell a
    // flipped condition from a correct one.
    let existing = std::env::current_exe().expect("the test binary has a path");
    let absent = Path::new("/nonexistent/tungsten1");

    assert_eq!(
        preflight(Path::new("/nonexistent/test.tg"), &existing),
        Err(MissingInput::SourceFile)
    );
    assert_eq!(
        preflight(&existing, absent),
        Err(MissingInput::SelfhostBinary)
    );
    // A missing source wins even when the binary is missing too — the
    // source is checked first.
    assert_eq!(
        preflight(Path::new("/nonexistent/test.tg"), absent),
        Err(MissingInput::SourceFile)
    );
    assert_eq!(preflight(&existing, &existing), Ok(()));
}

#[test]
fn missing_selfhost_binary_fails_before_running_anything() {
    let existing = std::env::current_exe().expect("the test binary has a path");
    let result =
        cmd_diff_bootstrap_selfhost_check(&existing, Path::new("/nonexistent/tungsten1"), false);
    assert_ne!(result, ExitCode::SUCCESS);
}

#[test]
fn agreement_reports_say_so() {
    let (clean, _) = render_report(&[], &[], false);
    assert!(clean.contains("agree: 0 errors"), "{clean}");

    let errs = strings(&["error[E0001]: boom"]);
    let (identical, _) = render_report(&errs, &errs, false);
    assert!(identical.contains("identical errors (1)"), "{identical}");
}

#[test]
fn selfhost_only_list_is_capped_with_an_overflow_tail() {
    // 25 self-host-only errors: exactly SELFHOST_LIST_CAP render, and the
    // remaining 5 are acknowledged rather than silently dropped.
    let selfhost: Vec<String> = (0..25).map(|i| format!("error: s{i}")).collect();
    let (text, code) = render_report(&[], &selfhost, false);
    assert_ne!(code, ExitCode::SUCCESS);
    assert!(text.contains("25 error(s) only in self-host"), "{text}");
    assert!(text.contains("  20. error: s19\n"), "{text}");
    assert!(!text.contains("21. error: s20"), "{text}");
    assert!(text.contains("... and 5 more"), "{text}");
}

#[test]
fn a_list_exactly_at_the_cap_gets_no_overflow_tail() {
    // Boundary: `> cap`, not `>= cap` — 20 entries print in full with no
    // "and 0 more" line.
    let selfhost: Vec<String> = (0..SELFHOST_LIST_CAP)
        .map(|i| format!("error: s{i}"))
        .collect();
    let (text, _) = render_report(&[], &selfhost, false);
    assert!(text.contains("  20. error: s19\n"), "{text}");
    assert!(!text.contains("more"), "{text}");
}

#[test]
fn bootstrap_only_renders_only_under_verbose_and_is_capped() {
    let bootstrap: Vec<String> = (0..15).map(|i| format!("error: b{i}")).collect();

    let (quiet, quiet_code) = render_report(&bootstrap, &[], false);
    assert!(!quiet.contains("only in bootstrap"), "{quiet}");

    let (loud, loud_code) = render_report(&bootstrap, &[], true);
    assert!(loud.contains("15 error(s) only in bootstrap"), "{loud}");
    assert!(loud.contains("  10. error: b9\n"), "{loud}");
    assert!(!loud.contains("11. error: b10"), "{loud}");

    // --verbose changes what is shown, never the verdict.
    assert_ne!(quiet_code, ExitCode::SUCCESS);
    assert_ne!(loud_code, ExitCode::SUCCESS);
}

#[test]
fn the_summary_line_counts_both_totals_and_both_differences() {
    let bootstrap = strings(&["error: shared", "error: b-only"]);
    let selfhost = strings(&["error: shared", "error: s-only"]);
    let (text, _) = render_report(&bootstrap, &selfhost, false);
    assert!(
        text.contains(
            "Summary: bootstrap=2 errors, self-host=2 errors, self-host-only=1, bootstrap-only=1"
        ),
        "{text}"
    );
}

#[test]
fn numbered_is_one_based_and_stops_at_the_limit() {
    let items = strings(&["a", "b", "c"]);
    let refs: Vec<&String> = items.iter().collect();
    assert_eq!(numbered(&refs, 2), "  1. a\n  2. b\n");
    assert_eq!(numbered(&refs, 9), "  1. a\n  2. b\n  3. c\n");
    assert_eq!(numbered(&refs, 0), "");
}

#[test]
fn both_error_spellings_are_recognised_independently() {
    // The predicate is an OR of two independent spellings — each must match
    // on its own, or a coded-only (or bare-only) run is silently dropped.
    assert!(is_error_line("error[E0001]: mismatch"));
    assert!(is_error_line("error: no main function"));
    assert!(!is_error_line("warning: unused"));
    assert!(!is_error_line("   Compiling tungsten"));
}

#[test]
fn collect_error_lines_filters_and_orders_stderr_first() {
    let errors = collect_error_lines(
        "error: from stderr\nnote: ignored\n",
        "error[E0001]: from stdout\nCompiling\n",
    );
    assert_eq!(
        errors,
        vec![
            "error: from stderr".to_string(),
            "error[E0001]: from stdout".to_string(),
        ]
    );
}

#[test]
fn run_check_captures_exit_code_and_only_error_lines() {
    // `echo` exits 0 and prints no error-shaped line — a cheap, hermetic
    // stand-in for a compiler binary that pins the exit-code capture.
    let echo = Path::new("/bin/echo");
    if echo.exists() {
        let (code, errors) = run_check(echo, Path::new("hello"), true).expect("echo runs");
        assert_eq!(code, 0);
        assert!(errors.is_empty(), "no error-shaped line: {errors:?}");
    }
}

#[test]
fn run_check_reports_a_binary_it_cannot_spawn() {
    let err = run_check(Path::new("/nonexistent/compiler"), Path::new("x.tg"), false)
        .expect_err("spawning a missing binary must fail");
    assert!(err.contains("/nonexistent/compiler"), "{err}");
}

#[test]
fn errors_missing_from_keeps_only_the_unmatched_entries() {
    let theirs = strings(&["a", "b", "c"]);
    let mine = strings(&["b"]);
    let only: Vec<&String> = errors_missing_from(&theirs, &mine);
    assert_eq!(only, vec![&theirs[0], &theirs[2]]);
    // Nothing is "missing" from an identical list.
    assert!(errors_missing_from(&theirs, &theirs).is_empty());
    // An empty `mine` keeps everything, in order.
    assert_eq!(errors_missing_from(&theirs, &[]).len(), 3);
}
