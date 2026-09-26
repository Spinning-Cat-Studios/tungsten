//! Each `doctor check type integrity` path reaches the check it names
//! (ADR 15.8.26a).
//!
//! The parse tests in `doctor/cli_tests/` prove every grouped spelling and
//! every hidden alias *resolves*; they cannot prove either one is wired to the
//! right handler. A regrouping is exactly the change that can misroute one —
//! four variants moved into a new enum, four arms rewritten, and the two
//! constructor checks are near-synonyms by name. Swap `type-stubs` and
//! `constructor-stubs` in the dispatch and every existing test stays green.
//!
//! So: run each path for real and assert on the line only *that* check emits.
//! Spawning the binary is the only way to observe it — the dispatchers return
//! `ExitCode`, which implements no accessor and no `PartialEq`, the same reason
//! `vacuous_mu_exit_codes.rs` and `termination_exit_codes.rs` spawn.
//!
//! Fixtures go in a tempdir rather than `tests/golden/`, so the assertions hold
//! in a mutation sweep's copied workspace as well as in the live checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One ADT with two constructors: enough for all four checks to have something
/// to say, and small enough that each says it in one line.
const ONE_ADT: &str = "\
type Colour = Red | Green

fn main() -> Nat { 0 }
";

/// The output fragment unique to each check, keyed by its subcommand.
///
/// Uniqueness is the whole assertion, and it is checked rather than assumed:
/// [`the_fingerprints_are_actually_unique`] fails if two of these ever match
/// the same run, which would silently turn every wiring assertion into a
/// tautology.
const FINGERPRINTS: &[(&str, &str)] = &[
    ("type-stubs", "stub resolution"),
    ("constructor-stubs", "no stale stubs"),
    ("constructor-counts", "2 entries"),
    ("phase-invariants", "All phase invariants hold"),
];

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("colour.tg");
    std::fs::write(&path, ONE_ADT).expect("write fixture");
    (dir, path)
}

/// Run `tungsten doctor check <args…> <path>`, returning status and output.
fn run(args: &[&str], path: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check"])
        .args(args)
        .arg(path)
        .output()
        .expect("spawn tungsten");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), combined)
}

#[test]
fn every_grouped_path_reaches_the_check_it_names() {
    let (_dir, path) = fixture();
    for (sub, fingerprint) in FINGERPRINTS {
        let (ok, output) = run(&["type", "integrity", sub], &path);
        assert!(
            ok,
            "`integrity {sub}` on a healthy file must pass: {output}"
        );
        assert!(
            output.contains(fingerprint),
            "`integrity {sub}` did not run the check it names \
             (looking for {fingerprint:?}): {output}"
        );
    }
}

/// The hidden aliases are not merely parseable — they reach the same check.
///
/// An alias that parses and dispatches elsewhere is worse than one that fails:
/// it appears in no `--help`, so nothing points a confused caller at the
/// discrepancy.
#[test]
fn every_hidden_alias_reaches_the_same_check_as_its_grouped_path() {
    let (_dir, path) = fixture();
    // `type-stubs` is spelled `stubs` flat — the one alias that is also a
    // rename, and therefore the one whose misrouting is most plausible.
    let pairs: &[(&str, &str)] = &[
        ("type-stubs", "stubs"),
        ("constructor-stubs", "constructor-stubs"),
        ("constructor-counts", "constructor-counts"),
        ("phase-invariants", "phase-invariants"),
    ];
    for (grouped, flat) in pairs {
        let (_, via_group) = run(&["type", "integrity", grouped], &path);
        let (_, via_alias) = run(&["type", flat], &path);
        assert_eq!(
            via_group, via_alias,
            "`type integrity {grouped}` and `type {flat}` must be the same check"
        );
    }
}

/// The `doctor check <name>` layer is a SECOND alias generation, and it does
/// not share a dispatcher with the first.
///
/// `doctor check stubs` predates the `check type` grouping entirely, so it is
/// matched in `doctor/dispatch.rs` and calls the check function directly —
/// never reaching `dispatch_check_integrity`. Three routes now converge on each
/// of these checks and only two of them are wired together, so a regrouping can
/// leave this one pointing at the old handler with every other test green.
///
/// `constructor-stubs` is absent on purpose: it never had a `check`-level
/// spelling, and inventing one here would assert a path the CLI does not offer.
#[test]
fn the_check_level_aliases_reach_the_same_check_as_the_grouped_path() {
    let (_dir, path) = fixture();
    let pairs: &[(&str, &str)] = &[
        ("type-stubs", "stubs"),
        ("constructor-counts", "constructor-counts"),
        ("phase-invariants", "phase-invariants"),
    ];
    for (grouped, flat) in pairs {
        let (_, via_group) = run(&["type", "integrity", grouped], &path);
        let (_, via_check) = run(&[flat], &path);
        assert_eq!(
            via_group, via_check,
            "`type integrity {grouped}` and `check {flat}` must be the same check"
        );
    }
}

/// The `check`-level layer reaches the exit status too.
///
/// It has its own dispatcher, so the `ExitCode::default()` mutant that
/// [`a_file_that_does_not_exist_reaches_the_exit_status`] kills on the grouped
/// path survives independently here.
#[test]
fn a_check_level_alias_on_a_missing_file_reaches_the_exit_status() {
    let missing = Path::new("/nonexistent-directory-for-tungsten-tests/missing.tg");
    for flat in ["stubs", "constructor-counts", "phase-invariants"] {
        let (ok, output) = run(&[flat], missing);
        assert!(
            !ok,
            "`check {flat}` reported success on a file it could not read: {output}"
        );
    }
}

/// A file that does not exist must reach the exit status, on every path.
///
/// Two things ride on this. The obvious one: a diagnostic that exits 0 on a
/// file it never read is the "0 examined reads like 0 violations" failure —
/// CI would go green on a typo'd path. The less obvious one: it is the only
/// assertion here that can fail when `dispatch_check_integrity` is replaced
/// wholesale by `ExitCode::default()`. Every other test runs a healthy fixture,
/// where success IS the expected status, so a dispatcher that does nothing and
/// returns 0 passes them all — a real surviving mutant until this test existed.
///
/// The four disagree on *which* non-zero code (1 vs 2), and that is not
/// asserted: it predates the grouping and is not this ADR's to settle.
#[test]
fn a_file_that_does_not_exist_reaches_the_exit_status() {
    let missing = Path::new("/nonexistent-directory-for-tungsten-tests/missing.tg");
    for (sub, _) in FINGERPRINTS {
        let (ok, output) = run(&["type", "integrity", sub], missing);
        assert!(
            !ok,
            "`integrity {sub}` reported success on a file it could not read: {output}"
        );
    }
}

/// A fingerprint that matched two checks would make the tests above pass no
/// matter how the dispatch is wired.
#[test]
fn the_fingerprints_are_actually_unique() {
    let (_dir, path) = fixture();
    for (sub, _) in FINGERPRINTS {
        let (_, output) = run(&["type", "integrity", sub], &path);
        let matched: Vec<&str> = FINGERPRINTS
            .iter()
            .filter(|(_, f)| output.contains(f))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            matched,
            vec![*sub],
            "`integrity {sub}` output matches more than one fingerprint, so the \
             wiring assertions above prove nothing: {output}"
        );
    }
}
