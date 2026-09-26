//! `doctor check type vacuous-mu`'s *exit code* (ADR 11.8.26c retrospective).
//!
//! `ExitCode` implements neither `PartialEq` nor any accessor, so the number a
//! shell sees can only be asserted by spawning the binary — the same reason
//! `termination_exit_codes.rs` exists, and the same reason a unit test cannot
//! kill a `-> ExitCode with Default::default()` mutant. It is the number that
//! matters: a census whose verdict never reaches the exit status is a check CI
//! cannot fail on, and this one's whole purpose is to fail *before* anyone
//! writes the `match` that would turn the shape into an E0064.
//!
//! Fixtures are written into a tempdir rather than read from `tests/golden/`,
//! so the assertions hold in a copied workspace (a mutation sweep) as well as
//! in the live checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A nested inductive family: the recursive occurrence sits under `Wrap`, so
/// `Rose` encodes to the vacuous `μα_Rose. α_Rose`.
///
/// Deliberately WITHOUT a `match` on `Rose` — that is the entire point. The
/// file elaborates cleanly and `tungsten check` is happy; only this census
/// notices. A fixture that matched would be rejected E0064 first and would
/// test the gate rather than the check.
const NESTED_FAMILY: &str = "\
type Wrap<T> = W(T)
type Rose = Node(Wrap<Rose>)

fn main() -> Nat { 0 }
";

/// The documented workaround, and the false positive this check shipped with:
/// `Rose` encodes to `μα_RoseKids. α_RoseKids`, a mutual-group MARKER for a
/// sibling that has a real body. It compiles, it matches, and it must not be
/// reported.
const MUTUAL_PAIR: &str = "\
type RoseKids = NoKids | Kid(Rose, RoseKids)
type Rose = Node(RoseKids)

fn kid_count(k: RoseKids) -> Nat {
    match k { NoKids() => 0, Kid(_, rest) => 1 + kid_count(rest) }
}

fn main() -> Nat { 0 }
";

/// Write `source` into a fresh tempdir and return it with the file's path.
fn fixture(name: &str, source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, source).expect("write fixture");
    (dir, path)
}

/// Run `tungsten doctor check type vacuous-mu <path>`, returning its status
/// and combined output.
fn census(path: &Path) -> (std::process::ExitStatus, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "type", "vacuous-mu"])
        .arg(path)
        .output()
        .expect("spawn tungsten");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status, combined)
}

#[test]
fn a_vacuous_encoding_reaches_the_exit_status() {
    let (_dir, path) = fixture("nested.tg", NESTED_FAMILY);
    let (status, output) = census(&path);

    assert!(
        !status.success(),
        "a vacuous μ must fail the check, not merely be printed: {output}"
    );
    assert!(output.contains("Rose"), "{output}");
    assert!(output.contains("α_Rose"), "{output}");
}

#[test]
fn a_healthy_project_exits_zero() {
    let (_dir, path) = fixture("mutual.tg", MUTUAL_PAIR);
    let (status, output) = census(&path);

    assert!(
        status.success(),
        "the documented E0064 workaround must not be reported: {output}"
    );
}

/// The file the census refuses still *checks* clean — which is the whole
/// argument for having the census at all. If `tungsten check` ever started
/// rejecting this, the check would be redundant and this test says so.
#[test]
fn the_refused_file_still_type_checks() {
    let (_dir, path) = fixture("nested.tg", NESTED_FAMILY);
    let out = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("spawn tungsten");

    assert!(
        out.status.success(),
        "a nested family's DEFINITION is accepted — the rejection is on `match`. \
         If this fails, `vacuous-mu` no longer earns its place: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
