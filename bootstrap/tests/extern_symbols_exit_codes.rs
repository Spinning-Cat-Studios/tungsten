//! `doctor check link extern-symbols`'s *exit codes* (ADR 18.8.26b retrospective).
//!
//! `ExitCode` implements neither `PartialEq` nor any accessor, so the number a
//! shell sees can only be asserted by spawning the binary — and it is the number
//! that matters: this check exists to fail a pre-flight, and a verdict that
//! never reaches the exit status is one CI cannot act on.
//!
//! The unit tests next to the check cover the decision (`verdict_of`) and the
//! wording (`render`). What only a spawn can cover is the mapping from that
//! decision onto a process exit — including the two arms where the finding is
//! about the *invocation* rather than the corpus, and where a wrong mapping
//! would report success over an examination that never happened.
//!
//! Fixtures are written into a tempdir rather than read from the checkout, so
//! the assertions hold in a copied workspace (a mutation sweep) as well as in
//! the live tree.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Declares one extern that `tungsten_core` provides and one it does not.
const ONE_MISSING: &str = "\
pub extern \"C\" fn tg_type_nat() -> Nat
pub extern \"C\" fn tg_no_such_symbol_anywhere(x: Nat) -> Nat

fn main() -> Nat { 0 }
";

/// Declares only externs that `tungsten_core` provides.
const ALL_PRESENT: &str = "\
pub extern \"C\" fn tg_type_nat() -> Nat
pub extern \"C\" fn tg_type_bool() -> Nat

fn main() -> Nat { 0 }
";

/// No `extern \"C\"` at all — nothing to examine.
const NO_EXTERNS: &str = "fn main() -> Nat { 0 }\n";

fn fixture(name: &str, source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, source).expect("write fixture");
    (dir, path)
}

/// The repo root, so `--core-root` resolves regardless of the test's cwd.
fn core_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("tungsten_core/src")
}

/// Run the check on `path` and return its exit status.
fn run(path: &Path, core: &Path) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "link", "extern-symbols"])
        .arg(path)
        .arg("--core-root")
        .arg(core)
        .output()
        .expect("spawn tungsten")
        .status
}

#[test]
fn a_declaration_with_no_export_reaches_the_exit_status() {
    let (_dir, path) = fixture("missing.tg", ONE_MISSING);

    assert!(
        !run(&path, &core_root()).success(),
        "an unresolved extern must fail the check, not merely print"
    );
}

#[test]
fn a_corpus_where_everything_resolves_exits_zero() {
    let (_dir, path) = fixture("present.tg", ALL_PRESENT);

    assert!(
        run(&path, &core_root()).success(),
        "a clean corpus must not fail the pre-flight"
    );
}

/// The arm that would otherwise look like success. A file with no declarations
/// examined nothing, so a zero exit would report "clean" about a run that
/// compared nothing — the failure this check is meant to catch in other tools.
#[test]
fn examining_nothing_is_a_failure_not_a_clean_run() {
    let (_dir, path) = fixture("bare.tg", NO_EXTERNS);

    assert!(
        !run(&path, &core_root()).success(),
        "0 examined must not exit like 0 findings"
    );
}

/// A `--core-root` that is not a directory is a finding about the *flag*, and
/// must not be reported as every declaration being unresolved.
#[test]
fn a_core_root_that_is_not_a_directory_fails_loudly() {
    let (_dir, path) = fixture("present.tg", ALL_PRESENT);
    let missing = Path::new("this/directory/does/not/exist");

    let output = Command::new(env!("CARGO_BIN_EXE_tungsten"))
        .args(["doctor", "check", "link", "extern-symbols"])
        .arg(&path)
        .arg("--core-root")
        .arg(missing)
        .output()
        .expect("spawn tungsten");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--core-root"), "{stderr}");
}

/// An unreadable input file is a hard failure, distinct from a finding.
#[test]
fn an_unreadable_input_file_fails_rather_than_reporting_zero_findings() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let absent = dir.path().join("nope.tg");

    assert!(!run(&absent, &core_root()).success());
}
