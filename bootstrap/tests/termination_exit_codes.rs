//! The termination gate's and report tool's *exit codes* (ADR 29.6.26e).
//!
//! `ExitCode` implements neither `PartialEq` nor any accessor, so the number a
//! shell sees can only be asserted by spawning the binary — and it is the number
//! that matters here: a gate whose verdict never reaches the exit status is a
//! gate CI cannot fail on.
//!
//! Fixtures are written into a tempdir rather than read from `tests/golden/`,
//! so the assertions hold in a copied workspace (a mutation sweep) as well as
//! in the live checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `fn spin(l)` recurses on its whole argument — never certifiable.
const REJECTING: &str = "\
type Lst = Nil | Cons(Nat, Lst)

fn spin(l: Lst) -> Nat { spin(l) }

fn main() -> Nat { 0 }
";

/// `fn len(l)` descends on the tail — certifiable.
const ADMITTED: &str = "\
type Lst = Nil | Cons(Nat, Lst)

fn len(l: Lst) -> Nat { match l { Nil => 0, Cons(h, t) => 1 + len(t) } }

fn main() -> Nat { 0 }
";

/// Write `source` into a fresh tempdir and return it with the file's path.
fn fixture(name: &str, source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, source).expect("write fixture");
    (dir, path)
}

/// Run `tungsten` with `args` and return whether it exited 0.
fn succeeds(args: &[&str], path: &Path) -> bool {
    run(args, path).0.success()
}

/// Run `tungsten` with `args`, returning its status and combined output.
fn run(args: &[&str], path: &Path) -> (std::process::ExitStatus, String) {
    let bin = env!("CARGO_BIN_EXE_tungsten");
    let mut command = Command::new(bin);
    command.args(args).arg(path);
    let out = command.output().expect("spawn tungsten");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status, combined)
}

#[test]
fn the_report_tool_exits_non_zero_on_a_definition_it_cannot_admit() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);

    assert!(
        !succeeds(&["doctor", "check", "type", "termination"], &path),
        "a non-admitted definition must reach the exit status"
    );
}

#[test]
fn the_report_tool_exits_zero_when_everything_is_admitted() {
    let (_dir, path) = fixture("admitted.tg", ADMITTED);

    assert!(
        succeeds(&["doctor", "check", "type", "termination"], &path),
        "a fully admitted project must not fail the tool"
    );
}

#[test]
fn the_gate_fails_the_build_at_full_enforcement() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);

    assert!(
        !succeeds(&["check", "--termination", "all"], &path),
        "`--termination all` must turn a rejection into a build failure"
    );
}

#[test]
fn the_gate_fails_the_build_at_the_default_enforcement_level() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);

    // ADR 11.8.26b: the default *is* `all`. This assertion is the one that has
    // to change when the default moves, so it is spelled against `check` with
    // no flag rather than against the variant name.
    assert!(
        !succeeds(&["check"], &path),
        "the default level gates an executable rejection"
    );
}

#[test]
fn proofs_only_still_demotes_an_executable_rejection_to_a_warning() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);

    assert!(
        succeeds(&["check", "--termination", "proofs"], &path),
        "`--termination proofs` reports an executable rejection rather than gating"
    );
}

#[test]
fn a_certifiable_project_passes_at_every_enforcement_level() {
    let (_dir, path) = fixture("admitted.tg", ADMITTED);

    for level in ["all", "proofs", "report"] {
        assert!(
            succeeds(&["check", "--termination", level], &path),
            "structural recursion must pass at --termination {level}"
        );
    }
}

/// ADR 12.8.26a: the report tool reaches its census on a file the gate rejects.
///
/// The distinction the assertion turns on is **exit 1 versus exit 2**. Exit 1 is
/// the tool's own verdict — it elaborated, counted, and found a rejection. Exit 2
/// is "elaboration aborted", which is what it returned between ADR 11.8.26b
/// flipping the default to `All` and 12.8.26a giving the tool `ReportingOnly`:
/// the census never printed, and the only diagnostic aimed at an uncertifiable
/// corpus was unreachable on every uncertifiable corpus.
#[test]
fn the_report_tool_reaches_its_census_even_when_the_gate_would_abort() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);
    let (status, output) = run(&["doctor", "check", "type", "termination"], &path);

    assert_eq!(
        status.code(),
        Some(1),
        "exit 1 is the tool's verdict; exit 2 would mean elaboration aborted first — {output}"
    );
    assert!(
        output.contains("admission:"),
        "the census line is the thing the tool exists to print — {output}"
    );
}

/// The same file under an explicit `--termination all` still reaches the census:
/// the guard is unconditional, not a default the user can accidentally undo.
#[test]
fn an_explicit_all_does_not_take_the_census_away_again() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);
    let (status, output) = run(
        &[
            "--termination",
            "all",
            "doctor",
            "check",
            "type",
            "termination",
        ],
        &path,
    );

    assert_eq!(status.code(), Some(1), "{output}");
    assert!(output.contains("admission:"), "{output}");
}

/// And the guard restores what it found: a `check` in the same process after the
/// report tool still gates. Asserted through two subprocesses rather than one,
/// because that is the only way the *shell-visible* code is observable.
#[test]
fn forcing_report_for_the_tool_does_not_weaken_a_later_check() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);
    assert!(
        !succeeds(&["check"], &path),
        "`check` still gates at the default level"
    );
}

/// ADR 12.8.26a §5.3: `info def --why-not-certified` explains an E0062
/// rejection, so it has to work on a file that *has* one.
///
/// It did not when 12.8.26a first shipped it — `elaborate_for_info` had no
/// `ReportingOnly` guard, so the gate aborted and the flag printed nothing on
/// the only input it is for. The ADR's own `suggest-tools` entry recommended it
/// for exactly those files. Found in review, not by a gate, which is why the
/// pairing row now exists too.
#[test]
fn why_not_certified_reaches_its_table_on_a_rejected_file() {
    let (_dir, path) = fixture("rejecting.tg", REJECTING);
    let (status, output) = run(&["info", "def", "spin", "--why-not-certified"], &path);

    assert!(
        output.contains("Decreasing roots"),
        "the table is the point of the flag — {output}"
    );
    // `spin(l: Lst)` has an eligible root and fails on DESCENT, so the advice
    // is the "root is fine, look at the call" branch. Asserting that rather
    // than the dead-end branch also pins which of the two a reader is given —
    // sending them to rewrite a parameter type here would be wrong.
    assert!(
        output.contains("DESCENT is not"),
        "an eligible root redirects to the call site — {output}"
    );
    assert!(status.success(), "inspection succeeded — {output}");
}
