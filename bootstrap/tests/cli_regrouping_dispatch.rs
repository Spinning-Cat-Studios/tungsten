//! Each path ADR 19.8.26a moved reaches the command it names.
//!
//! The parse tests in `cli/tests.rs` prove every grouped spelling and every
//! hidden alias *resolves*; they cannot prove either one is wired to the right
//! handler. A regrouping is exactly the change that can misroute one — variants
//! moved into new enums, arms rewritten, and `info type members` holds two
//! near-synonyms (`field-type` and `record-fields`) whose swap no parse test
//! could see.
//!
//! So: run each path for real and assert on the line only *that* command emits.
//! Spawning the binary is the only way to observe it — the dispatchers return
//! `ExitCode`, which implements no accessor and no `PartialEq`, the same reason
//! `check_type_integrity_dispatch.rs` spawns.
//!
//! Fixtures go in a tempdir rather than `tests/golden/`, so the assertions hold
//! in a mutation sweep's copied workspace as well as in the live checkout — and
//! so `cache clean-project`, which deletes, is pointed at a throwaway project.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One ADT and one record: enough for all four `info type members` views to
/// have something to say, and small enough that each says it in one line.
const ONE_ADT_ONE_RECORD: &str = "\
type Colour = Red | Green(Nat)

type Point = { x: Nat, y: Nat }

fn main() -> Nat { 0 }
";

/// The `info type members` subcommand, its operand, and the output fragment
/// unique to it.
///
/// Uniqueness is the whole assertion, and it is checked rather than assumed:
/// [`the_member_fingerprints_are_actually_unique`] fails if two of these ever
/// match the same run, which would silently turn every wiring assertion below
/// into a tautology.
const MEMBER_VIEWS: &[(&str, &str, &str)] = &[
    ("constructors", "Colour", "Constructor entries:"),
    ("visibility", "Colour", "Declared visibility:"),
    ("record-fields", "Point", "Product encoding:"),
    ("field-type", "Point.x", "Field: Point.x"),
];

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("shape.tg");
    std::fs::write(&path, ONE_ADT_ONE_RECORD).expect("write fixture");
    (dir, path)
}

/// Run `tungsten <args…>`, returning success and combined output.
fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn tungsten");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), combined)
}

fn run_member(prefix: &[&str], sub: &str, operand: &str, file: &Path) -> (bool, String) {
    let file = file.to_string_lossy().into_owned();
    let mut args: Vec<&str> = prefix.to_vec();
    args.extend_from_slice(&[sub, operand, &file]);
    run(&args)
}

// ── `info type members` ──

#[test]
fn every_member_path_reaches_the_view_it_names() {
    let (_dir, path) = fixture();
    for (sub, operand, fingerprint) in MEMBER_VIEWS {
        let (ok, output) = run_member(&["info", "type", "members"], sub, operand, &path);
        assert!(ok, "`members {sub}` on a healthy file must pass: {output}");
        assert!(
            output.contains(fingerprint),
            "`members {sub}` did not run the view it names \
             (looking for {fingerprint:?}): {output}"
        );
    }
}

/// The hidden aliases are not merely parseable — they reach the same view.
///
/// An alias that parses and dispatches elsewhere is worse than one that fails:
/// it appears in no `--help`, so nothing points a confused caller at the
/// discrepancy.
///
/// Each side runs **twice** and the second runs are compared. The first run of
/// either spelling populates the project's elaboration cache, so a cold-vs-warm
/// pair differs in ways that have nothing to do with the wiring — an equality
/// assertion on first runs fails on a correct implementation.
#[test]
fn every_hidden_alias_reaches_the_same_view_as_its_grouped_path() {
    let (_dir, path) = fixture();
    for (sub, operand, _) in MEMBER_VIEWS {
        run_member(&["info", "type", "members"], sub, operand, &path);
        run_member(&["info", "type"], sub, operand, &path);
        let (_, via_group) = run_member(&["info", "type", "members"], sub, operand, &path);
        let (_, via_alias) = run_member(&["info", "type"], sub, operand, &path);
        assert_eq!(
            via_group, via_alias,
            "`info type members {sub}` and `info type {sub}` must be the same view"
        );
    }
}

/// A fingerprint that matched two views would make the assertions above pass no
/// matter how the dispatch is wired.
#[test]
fn the_member_fingerprints_are_actually_unique() {
    let (_dir, path) = fixture();
    for (sub, operand, _) in MEMBER_VIEWS {
        let (_, output) = run_member(&["info", "type", "members"], sub, operand, &path);
        let matched: Vec<&str> = MEMBER_VIEWS
            .iter()
            .filter(|(_, _, f)| output.contains(f))
            .map(|(name, _, _)| *name)
            .collect();
        assert_eq!(
            matched,
            vec![*sub],
            "`members {sub}` output matches more than one fingerprint, so the \
             wiring assertions above prove nothing: {output}"
        );
    }
}

/// A file that does not exist must reach the exit status, on every path.
///
/// Two things ride on this. The obvious one: a diagnostic that exits 0 on a
/// file it never read is the "0 examined reads like 0 violations" failure — CI
/// would go green on a typo'd path. The less obvious one: it is the only
/// assertion here that can fail when `dispatch_type_members` is replaced
/// wholesale by `ExitCode::default()`, which is SUCCESS. Every other test runs
/// a healthy fixture, where success IS the expected status, so a dispatcher
/// that does nothing at all would pass them (ADR 15.8.26a §6.3).
#[test]
fn a_missing_file_reaches_the_exit_status_on_every_member_path() {
    let missing = Path::new("/nonexistent-directory-for-tungsten-tests/missing.tg");
    for (sub, operand, _) in MEMBER_VIEWS {
        let (ok, output) = run_member(&["info", "type", "members"], sub, operand, missing);
        assert!(
            !ok,
            "`members {sub}` reported success on a file it could not read: {output}"
        );
    }
}

// ── `expr` ──

/// `expr eval` and `expr repl` share an enum and differ by one variant, so the
/// plausible misroute sends `eval` to the REPL — which reads stdin and would
/// hang rather than fail. Asserting on the *evaluated value* is what
/// distinguishes them; a `--help` check could not.
#[test]
fn expr_eval_evaluates_and_its_hidden_alias_agrees() {
    let (ok, grouped) = run(&["expr", "eval", "1 + 2"]);
    assert!(ok, "`expr eval` failed: {grouped}");
    assert!(
        grouped.contains('3'),
        "`expr eval \"1 + 2\"` did not evaluate — it may be wired to the REPL: {grouped}"
    );
    let (ok, alias) = run(&["eval", "1 + 2"]);
    assert!(ok, "the hidden `eval` alias failed: {alias}");
    assert_eq!(
        grouped, alias,
        "`expr eval` and the flat `eval` must be the same evaluator"
    );
}

/// `expr eval` on a malformed expression must reach the exit status, for the
/// same `ExitCode::default()` reason as the member paths above.
#[test]
fn expr_eval_reaches_the_exit_status_on_a_malformed_expression() {
    let (ok, output) = run(&["expr", "eval", "1 +"]);
    assert!(
        !ok,
        "`expr eval` reported success on an expression it could not parse: {output}"
    );
}

// ── `cache clean-project` ──

/// The re-homed command must clear the project it is pointed at, and name the
/// root it cleared.
///
/// Naming the root is not decoration: with no operand the root is the *cwd*,
/// which is not where a build writes (ADR 5.8.26d D5), so a run that cleared
/// nothing and a run that cleared everything are otherwise indistinguishable.
/// The misroute this catches is `clean-project` wired to `cache clean`, which
/// would walk the tree instead.
#[test]
fn cache_clean_project_clears_the_project_it_is_pointed_at() {
    let (dir, path) = fixture();
    let file = path.to_string_lossy().into_owned();
    let (ok, _) = run(&["check", &file]);
    assert!(
        ok,
        "fixture must type-check before its cache can be cleared"
    );
    let cache = dir.path().join(".tungsten");
    assert!(cache.exists(), "`check` did not write a cache to clear");

    let (ok, output) = run(&["cache", "clean-project", &file]);
    assert!(ok, "`cache clean-project` failed: {output}");
    assert!(
        output.contains(&*dir.path().to_string_lossy()),
        "`cache clean-project` did not report the root it cleared: {output}"
    );
}

/// The hidden `clean` alias reaches the same handler.
#[test]
fn the_hidden_clean_alias_reaches_cache_clean_project() {
    let (dir, path) = fixture();
    let file = path.to_string_lossy().into_owned();
    run(&["check", &file]);
    let (ok, output) = run(&["clean", &file]);
    assert!(ok, "the hidden `clean` alias failed: {output}");
    assert!(
        output.contains(&*dir.path().to_string_lossy()),
        "the hidden `clean` alias did not clear the project it was given: {output}"
    );
}
