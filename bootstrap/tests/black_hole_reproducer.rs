//! Subprocess-isolated regression tests for ADR 22.7.26a — black-hole
//! detection in the evaluator.
//!
//! A self-referential 0-arg global (`fn f() -> Nat { f() }`) re-enters
//! `EvalEnv::lookup` before the memo is written, which pre-fix was unbounded
//! Rust recursion: the child overflows its stack in milliseconds and, on
//! macOS, wedges as an unkillable `UE` process mid-abort. These tests are
//! therefore subprocess-isolated — an in-process reproducer would abort (or
//! wedge) the whole test binary — and they wait on the child with a polling
//! deadline rather than a blocking `.output()`, so a future revert shows up
//! as a *failing test*, not a hung suite (ADR 22.7.26a AC 1).
//!
//! Runs LLVM-free: `tungsten test`/`tungsten run` use the evaluator path.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A body that reaches its own global in evaluation position. `loop_forever`
/// stores its body directly (0-arg), so forcing it re-enters `lookup`.
///
/// `#[partial]` since ADR 11.8.26b made enforcement `all` the default: the
/// elaborator would otherwise reject `loop_forever` before the evaluator ever
/// saw it, and these tests would pass by never reaching what they test. The
/// two gates are complementary — E0062 refuses to *certify* the recursion,
/// black-hole detection is what catches an opted-out one at run time.
const SELF_REFERENTIAL_TEST_FIXTURE: &str = "\
extern \"C\" fn tg_assert_eq_nat(left: Nat, right: Nat) -> Unit
fn assert_eq_nat(a: Nat, b: Nat) -> Unit { tg_assert_eq_nat(a, b) }

#[partial]
fn loop_forever() -> Nat { loop_forever() }

pub fn test_black_hole() -> Unit { assert_eq_nat(loop_forever(), 0) }
";

/// The same shape reaching `tungsten run` through `main` (ADR 22.7.26a §1.4).
const SELF_REFERENTIAL_RUN_FIXTURE: &str = "\
#[partial]
fn loop_forever() -> Nat { loop_forever() }

fn main() -> Nat { loop_forever() }
";

/// How long the child gets to exit before we call it wedged. Post-fix the
/// diagnostic path returns in well under a second; the margin absorbs a cold
/// filesystem or a loaded CI host.
const CHILD_EXIT_DEADLINE: Duration = Duration::from_secs(60);

/// Wait for the child with a polling deadline. Pre-fix the stack-overflow
/// abort can wedge the child in macOS `UE` state, where a blocking wait never
/// returns — polling keeps the *parent* suite alive. Returns the captured
/// stdout+stderr and the exit status, or panics (test failure) on a child
/// that never exits.
fn wait_with_deadline(mut child: Child) -> (String, std::process::ExitStatus) {
    let deadline = Instant::now() + CHILD_EXIT_DEADLINE;
    loop {
        match child.try_wait().expect("failed to poll child process") {
            Some(status) => {
                let mut combined = String::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_string(&mut combined).ok();
                }
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_string(&mut combined).ok();
                }
                return (combined, status);
            }
            None if Instant::now() >= deadline => {
                // Best-effort kill; a UE-wedged child ignores even SIGKILL.
                child.kill().ok();
                panic!(
                    "child did not exit within {CHILD_EXIT_DEADLINE:?} — \
                     likely the pre-fix stack-overflow wedge (ADR 22.7.26a §1.2)"
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Write `source` into a fresh tempdir (isolating the `.tungsten` cache) and
/// spawn the real CLI on it. Panics — loudly, per AC 1 — if the binary
/// cannot be spawned at all (the binary-skew class ADR 21.7.26f/D3 documents).
fn spawn_tungsten_on(source: &str, subcommand: &str) -> (tempfile::TempDir, Child) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let path = dir.path().join("prog.tg");
    std::fs::write(&path, source).expect("failed to write fixture");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tungsten"));
    command.arg(subcommand).arg(&path);
    if subcommand == "test" {
        // `run` has no --color flag; the test runner's needs disabling so the
        // BLACK HOLE assertion matches unstyled text.
        command.arg("--color").arg("never");
    }
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn tungsten binary — unrunnable binary must fail loudly, not skip");
    (dir, child)
}

/// `tungsten test` on the reproducer: exits nonzero *with* the black-hole
/// diagnostic — a named failure, not a crash and not a silent `ok`.
#[test]
fn test_runner_reports_black_hole_and_exits_nonzero() {
    let (_dir, child) = spawn_tungsten_on(SELF_REFERENTIAL_TEST_FIXTURE, "test");
    let (output, status) = wait_with_deadline(child);

    assert!(
        output.contains("BLACK HOLE"),
        "expected the BLACK HOLE diagnostic in the output; got:\n{output}"
    );
    assert!(
        output.contains("loop_forever"),
        "diagnostic must name the self-referential global; got:\n{output}"
    );
    assert!(
        !status.success(),
        "a black-holed test must fail the run; got {status:?} with output:\n{output}"
    );
    assert!(
        status.code().is_some(),
        "child must exit cleanly (diagnostic), not die on a signal; got {status:?}"
    );
}

/// `tungsten run` on the reproducer: a diagnostic and a nonzero exit instead
/// of a stack-overflow abort (ADR 22.7.26a §1.4 — not test-runner-specific).
#[test]
fn run_reports_black_hole_and_exits_nonzero() {
    let (_dir, child) = spawn_tungsten_on(SELF_REFERENTIAL_RUN_FIXTURE, "run");
    let (output, status) = wait_with_deadline(child);

    assert!(
        output.contains("black hole"),
        "expected a black-hole diagnostic from `tungsten run`; got:\n{output}"
    );
    assert!(
        output.contains("loop_forever"),
        "diagnostic must name the self-referential global; got:\n{output}"
    );
    assert!(
        !status.success(),
        "a black-holed run must exit nonzero; got {status:?} with output:\n{output}"
    );
    assert!(
        status.code().is_some(),
        "child must exit cleanly (diagnostic), not die on a signal; got {status:?}"
    );
}
