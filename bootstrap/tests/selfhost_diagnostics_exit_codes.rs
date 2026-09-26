//! Exit codes for the two self-host diagnostics (ADR 19.8.26d retrospective).
//!
//! `ExitCode` implements neither `PartialEq` nor any accessor, so the number a
//! shell sees can only be asserted by spawning the binary — and here the number
//! carries an unusual amount: **three of the outcomes are non-zero and two of
//! those mean "could not ask" rather than "found something".** Both tools exist
//! because a silent no-op read as a clean bill of health, so a mapping that
//! exited 0 on an unanswerable probe would rebuild the very defect they catch.
//!
//! The unit tests beside each tool cover the decision (`outcome_for`,
//! `compare`) and the parsing. What only a spawn can cover is the path from
//! that decision through the subprocess shell to a process exit — including
//! the "self-host is missing" and "self-host cannot answer" arms, which is
//! where these tools are used most, since a `tungsten1` built with diagnostics
//! is the exception rather than the rule.
//!
//! Fixtures go in a tempdir rather than the checkout, so the assertions hold in
//! a copied workspace (a mutation sweep) as well as in the live tree.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A file both compilers can elaborate.
const TRIVIAL: &str = "fn main() -> Nat { 0 }\n";

fn fixture(name: &str, source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, source).expect("write fixture");
    (dir, path)
}

/// A path that exists and is executable but is not a self-host compiler:
/// this test binary. Standing in for a `tungsten1` that cannot answer, which
/// is the arm both tools spend most of their time in.
fn not_a_selfhost() -> PathBuf {
    std::env::current_exe().expect("the test binary has a path")
}

/// A stand-in self-host that prints exactly `output` and exits 0.
///
/// A real `tungsten1` with diagnostics compiled in is a devcontainer artefact,
/// so without this the success paths — a clean census, a census with findings,
/// a rendered term — are unreachable from a host `cargo test`, and the only
/// arms covered would be the ones where nothing was learned. It also pins the
/// subprocess seam itself: a `probe` that returned canned text instead of
/// spawning would no longer agree with what the fake printed.
fn fake_selfhost(dir: &Path, output: &str) -> PathBuf {
    let path = dir.join("fake-tungsten1");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat <<'FAKE_EOF'\n{output}\nFAKE_EOF\n"),
    )
    .expect("write fake self-host");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake executable");
    }
    path
}

fn closed_terms(file: &Path, selfhost: &Path) -> Option<i32> {
    Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "closed-terms"])
        .arg(file)
        .arg("--selfhost-binary")
        .arg(selfhost)
        .output()
        .expect("spawn tungsten")
        .status
        .code()
}

fn well_typed_terms(file: &Path, selfhost: &Path) -> Option<i32> {
    Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "well-typed-terms"])
        .arg(file)
        .arg("--selfhost-binary")
        .arg(selfhost)
        .output()
        .expect("spawn tungsten")
        .status
        .code()
}

fn selfhost_core(definition: &str, file: &Path, selfhost: &Path) -> Option<i32> {
    Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["diff", "selfhost-core", definition])
        .arg(file)
        .arg("--selfhost-binary")
        .arg(selfhost)
        .output()
        .expect("spawn tungsten")
        .status
        .code()
}

#[test]
fn closed_terms_reports_two_when_the_selfhost_cannot_answer() {
    // Not 0. A probe that learned nothing must not read as "every term is
    // closed" — that is the failure mode the check was written to catch, and
    // the one it would itself commit if this mapping were wrong.
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    assert_eq!(
        closed_terms(&path, &not_a_selfhost()),
        Some(2),
        "an unanswerable probe is a failure, not a clean corpus"
    );
}

#[test]
fn closed_terms_fails_when_the_selfhost_binary_is_absent() {
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    let code = closed_terms(&path, Path::new("/nonexistent/tungsten1"));
    assert!(
        code.is_some_and(|c| c != 0),
        "a missing self-host must not exit 0, got {code:?}"
    );
}

#[test]
fn closed_terms_fails_when_the_source_is_absent() {
    let code = closed_terms(Path::new("/nonexistent/probe.tg"), &not_a_selfhost());
    assert!(
        code.is_some_and(|c| c != 0),
        "a missing source must not exit 0, got {code:?}"
    );
}

#[test]
fn selfhost_core_reports_two_when_the_selfhost_cannot_answer() {
    // The bootstrap side succeeds — `main` is there — so reaching 2 proves the
    // self-host half was consulted and its silence was believed rather than
    // read as agreement.
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    assert_eq!(
        selfhost_core("main", &path, &not_a_selfhost()),
        Some(2),
        "a self-host that rendered nothing is not agreement"
    );
}

#[test]
fn selfhost_core_fails_when_the_bootstrap_has_no_such_definition() {
    // A different failure from the one above, and it must not be mistaken for
    // a divergence: there is nothing to compare, so the comparison never runs.
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    let code = selfhost_core("no_such_definition", &path, &not_a_selfhost());
    assert!(
        code.is_some_and(|c| c != 0),
        "an unknown definition must not exit 0, got {code:?}"
    );
}

#[test]
fn selfhost_core_fails_when_the_selfhost_binary_is_absent() {
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    let code = selfhost_core("main", &path, Path::new("/nonexistent/tungsten1"));
    assert!(
        code.is_some_and(|c| c != 0),
        "a missing self-host must not exit 0, got {code:?}"
    );
}

// ── The success paths, through the real subprocess seam ──────────────────
//
// These need a self-host that ANSWERS. A `tungsten1` with diagnostics compiled
// in only exists inside the devcontainer, so the answer is faked — which is
// enough, because what is under test here is this binary's reading of a
// census, not the self-host's production of one.

#[test]
fn a_clean_census_exits_zero() {
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[free-vars] census: 2267 definition(s) examined, 0 with free variable(s)",
    );

    assert_eq!(closed_terms(&path, &fake), Some(0));
}

#[test]
fn a_census_with_findings_exits_one_and_names_them() {
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[free-vars] len2: h, t\n         [free-vars] census: 3 definition(s) examined, 1 with free variable(s)",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "closed-terms"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(1));
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(rendered.contains("len2"), "{rendered}");
    assert!(rendered.contains("h, t"), "{rendered}");
}

#[test]
fn a_census_that_examined_nothing_exits_two_not_zero() {
    // Textually a clean verdict; semantically no verdict. This is the arm the
    // whole check exists for, and the one a naive mapping gets wrong.
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[free-vars] census: 0 definition(s) examined, 0 with free variable(s)",
    );

    assert_eq!(closed_terms(&path, &fake), Some(2));
}

#[test]
fn a_stubbed_selfhost_is_reported_as_such_rather_than_as_clean() {
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[diagnostics] this binary has no diagnostic tools compiled in, so the\n         requested flag(s) did nothing.",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "closed-terms"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("self-compile-dev"),
        "the remedy must reach the user, not just the exit code: {stderr}"
    );
}

#[test]
fn agreeing_core_terms_exit_zero() {
    // `main` in TRIVIAL elaborates to `zero` under the bootstrap. A fake that
    // reports the same term must compare equal — which also pins that the
    // comparison reads the self-host's term rather than assuming agreement.
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(dir.path(), "[core] main\tzero\n[dump-core] 1 of 1 matched");

    assert_eq!(selfhost_core("main", &path, &fake), Some(0));
}

#[test]
fn differing_core_terms_exit_one_and_print_both() {
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[core] main\tsucc zero\n[dump-core] 1 of 1 matched",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["diff", "selfhost-core", "main"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(1));
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(rendered.contains("succ zero"), "{rendered}");
    assert!(
        rendered.contains("bootstrap:") && rendered.contains("self-host:"),
        "both sides must be shown, not just the verdict: {rendered}"
    );
}

// ── well-typed-terms (ADR 3.9.26h AC4) ───────────────────────────────────
//
// The same four verdicts as `closed-terms`, and the same rule: a run that
// learned nothing must not read as a clean corpus. AC4 asks specifically that
// `0 examined` and `0 findings` be distinguishable, and that a production
// (diagnostics-stubbed) `tungsten1` say so — both are spawned here rather than
// verified by hand, because the exit code is what a gate would read.

#[test]
fn well_typed_terms_with_no_arguments_refuses_rather_than_exiting_zero() {
    // `file` is a required positional, so the parser refuses. Asserted because
    // a subcommand that defaulted the path would examine some other corpus and
    // report a clean verdict about it.
    let code = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "well-typed-terms"])
        .output()
        .expect("spawn tungsten")
        .status
        .code();
    assert!(
        code.is_some_and(|c| c != 0),
        "no arguments must not exit 0, got {code:?}"
    );
}

/// The `--help` text of one `doctor check selfhost` subcommand.
fn selfhost_help(subcommand: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", subcommand, "--help"])
        .output()
        .expect("spawn tungsten");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_closed_terms_help_no_longer_claims_descent_refused_282_definitions() {
    // ADR 3.9.26h D5. The claim was present-tense and stale: ADR 21.8.26a
    // emitted the missing projections, so descent no longer refuses them and
    // the number is wrong in both magnitude and cause. `--help` is the fifth
    // agent surface and the one nothing reconciles, so this asserts it.
    let help = selfhost_help("closed-terms");
    assert!(
        !help.contains("refused 282"),
        "the stale claim is back: {help}"
    );
    assert!(
        help.contains("21.8.26a"),
        "the correction must say what actually happened: {help}"
    );
    assert!(
        help.contains("well-typed-terms"),
        "each half must point at the other: {help}"
    );
}

#[test]
fn the_well_typed_terms_help_says_which_formers_it_judges() {
    // A shape check that did not say WHICH shapes reads as a type checker, and
    // a reader who expects one will treat a clean verdict as far stronger than
    // it is.
    let help = selfhost_help("well-typed-terms");
    for former in ["Product", "arrow", "Sum", "Mu"] {
        assert!(help.contains(former), "{former} missing from: {help}");
    }
    assert!(help.contains("closed-terms"), "{help}");
}

#[test]
fn well_typed_terms_reports_two_when_the_selfhost_cannot_answer() {
    let (_dir, path) = fixture("trivial.tg", TRIVIAL);

    assert_eq!(
        well_typed_terms(&path, &not_a_selfhost()),
        Some(2),
        "an unanswerable probe is a failure, not a corpus of good shapes"
    );
}

#[test]
fn a_production_selfhost_says_it_was_stubbed_rather_than_reporting_clean() {
    // The production build stubs the diagnostics out (ADR 18.4.26f), so the
    // flag runs, finds nothing and prints nothing. Exit 2 plus the remedy.
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[diagnostics] this binary has no diagnostic tools compiled in, so the\n         requested flag(s) did nothing.",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "well-typed-terms"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("self-compile-dev"),
        "the remedy must reach the user, not just the exit code: {stderr}"
    );
}

#[test]
fn a_shape_census_that_examined_nothing_exits_two_and_a_clean_one_exits_zero() {
    // AC4's whole point, as one assertion pair: the two censuses differ only in
    // the denominator, and they must NOT render alike.
    let (dir, path) = fixture("trivial.tg", TRIVIAL);

    let empty = fake_selfhost(
        dir.path(),
        "[well-typed] census: 0 definition(s) examined, 0 with shape mismatch(es)",
    );
    assert_eq!(well_typed_terms(&path, &empty), Some(2));

    let clean_dir = tempfile::tempdir().expect("create tempdir");
    let clean = fake_selfhost(
        clean_dir.path(),
        "[well-typed] census: 2298 definition(s) examined, 0 with shape mismatch(es)",
    );
    assert_eq!(well_typed_terms(&path, &clean), Some(0));
}

#[test]
fn a_shape_census_with_findings_exits_one_and_names_the_eliminators() {
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[well-typed] record_field: fst over Nat; snd over a recursive type\n         [well-typed] census: 3 definition(s) examined, 1 with shape mismatch(es)",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "selfhost", "well-typed-terms"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(1));
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(rendered.contains("record_field"), "{rendered}");
    assert!(rendered.contains("fst over Nat"), "{rendered}");
}

#[test]
fn an_unanswerable_comparison_tells_the_user_what_to_do() {
    // The guidance is the point of the unanswerable arms: the exit code says
    // "no", and only the message says why and what would fix it.
    let (dir, path) = fixture("trivial.tg", TRIVIAL);
    let fake = fake_selfhost(
        dir.path(),
        "[diagnostics] this binary has no diagnostic tools",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["diff", "selfhost-core", "main"])
        .arg(&path)
        .arg("--selfhost-binary")
        .arg(&fake)
        .output()
        .expect("spawn tungsten");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("self-compile-dev"),
        "the remedy must reach the user: {stderr}"
    );
}
