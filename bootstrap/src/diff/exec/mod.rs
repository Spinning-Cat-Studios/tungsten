//! `tungsten diff exec` — differential evaluator-vs-native execution
//! (ADR 3.7.26d §2.2).
//!
//! Compiles the program natively, runs it, runs the same program under the
//! bootstrap evaluator (`tungsten run`), and compares observable output. This is
//! the general detector for the silent-value-miscompile class (ADR 3.7.26a
//! defect 2): a binary that compiles cleanly, links cleanly, passes the
//! LLVM verifier, and prints the wrong value.
//!
//! **What is compared:** stdout, byte-for-byte after trailing-newline
//! normalization. The runtime prints `main`'s value to stdout and compiled
//! binaries always exit 0, so stdout parity subsumes value parity. stderr
//! is excluded from parity (evaluator diagnostics legitimately differ) but
//! included in the divergence report.
//!
//! **Exit codes (mutually exclusive):** 0 parity; 1 output divergence;
//! 2 native runtime error with evaluator Ok; 3 compile error (neither side
//! ran — also used for a missing input file); 4 evaluator error with native
//! Ok; 5 timeout on either side.
//!
//! **Eligibility (v1):** deterministic programs only — empty stdin, no
//! argv, environment not inherited by the native binary beyond PATH/TMPDIR.
//! Programs reading time, randomness, filesystem state, or the environment
//! can diverge without a compiler bug.
//!
//! **Testability hook:** the `TUNGSTEN_DIFF_EXEC_{COMPILE,NATIVE,EVAL}_OVERRIDE`
//! env vars (whitespace-split argv, test-only) replace the corresponding
//! step so divergence, runtime-failure, and timeout classes are testable
//! without a miscompiling compiler.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// Execution status of one side (§2.2 result contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecStatus {
    Ok,
    RuntimeError,
    Timeout,
}

/// Observable output of one side (§2.2 result contract).
#[derive(Debug, Clone)]
pub(crate) struct ExecRecord {
    pub(crate) status: ExecStatus,
    /// Full captured stdout (compared).
    pub(crate) stdout: String,
    /// Captured stderr — reported on divergence, NOT compared.
    pub(crate) stderr: String,
}

/// Outcome classes with their §2.2 exit codes (mutually exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Parity,
    Divergence,
    NativeRuntimeError,
    CompileError,
    EvalRuntimeError,
    Timeout,
}

impl Outcome {
    pub(crate) fn exit_code(self) -> u8 {
        match self {
            Outcome::Parity => 0,
            Outcome::Divergence => 1,
            Outcome::NativeRuntimeError => 2,
            Outcome::CompileError => 3,
            Outcome::EvalRuntimeError => 4,
            Outcome::Timeout => 5,
        }
    }
}

/// stdout parity ignores trailing newlines only (§2.2).
pub(crate) fn normalize(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// Classify two execution records into an outcome class. `CompileError` is
/// short-circuited before both sides run, so it never reaches here.
///
/// Both sides failing at runtime is status agreement — parity is then
/// decided by stdout like the Ok/Ok case.
pub(crate) fn classify(native: &ExecRecord, eval: &ExecRecord) -> Outcome {
    match (native.status, eval.status) {
        (ExecStatus::Timeout, _) | (_, ExecStatus::Timeout) => Outcome::Timeout,
        (ExecStatus::RuntimeError, ExecStatus::Ok) => Outcome::NativeRuntimeError,
        (ExecStatus::Ok, ExecStatus::RuntimeError) => Outcome::EvalRuntimeError,
        _ => {
            if normalize(&native.stdout) == normalize(&eval.stdout) {
                Outcome::Parity
            } else {
                Outcome::Divergence
            }
        }
    }
}

/// 1-indexed line number of the first divergence between two normalized
/// stdouts (compares line-by-line; a missing line diverges).
pub(crate) fn first_divergence_line(native_out: &str, eval_out: &str) -> usize {
    let mut native_lines = normalize(native_out).lines();
    let mut eval_lines = normalize(eval_out).lines();
    let mut line_no = 1;
    loop {
        match (native_lines.next(), eval_lines.next()) {
            (None, None) => return line_no,
            (left, right) if left == right => line_no += 1,
            _ => return line_no,
        }
    }
}

/// Test-only overrides for the three subprocess steps (ADR 3.7.26d §2.2
/// testability hook). Each is an argv replacing the corresponding step.
#[derive(Default)]
pub(crate) struct ExecOverrides {
    pub(crate) compile: Option<Vec<String>>,
    pub(crate) native: Option<Vec<String>>,
    pub(crate) eval: Option<Vec<String>>,
}

impl ExecOverrides {
    /// Read the hidden env-var hooks (whitespace-split argv; test-only).
    pub(crate) fn from_env() -> Self {
        ExecOverrides {
            compile: argv_from_env("TUNGSTEN_DIFF_EXEC_COMPILE_OVERRIDE"),
            native: argv_from_env("TUNGSTEN_DIFF_EXEC_NATIVE_OVERRIDE"),
            eval: argv_from_env("TUNGSTEN_DIFF_EXEC_EVAL_OVERRIDE"),
        }
    }
}

/// Read a whitespace-split argv override from env var `name` (test-only hook).
/// Returns `None` when unset or empty. Shared by `diff exec` and `diff cache`.
pub(crate) fn argv_from_env(name: &str) -> Option<Vec<String>> {
    std::env::var(name)
        .ok()
        .map(|v| v.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|argv| !argv.is_empty())
}

/// Entry point for `tungsten diff exec <file> [--timeout <secs>]`.
pub(crate) fn cmd_diff_exec(file: &Path, timeout_secs: u64, overrides: &ExecOverrides) -> ExitCode {
    let timeout = Duration::from_secs(timeout_secs);
    match run_diff_exec(file, timeout, overrides) {
        Ok(outcome) => ExitCode::from(outcome.exit_code()),
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(Outcome::CompileError.exit_code())
        }
    }
}

fn run_diff_exec(
    file: &Path,
    timeout: Duration,
    overrides: &ExecOverrides,
) -> Result<Outcome, String> {
    if !file.exists() {
        return Err(format!(
            "source file not found: {} (exit 3: neither side ran)",
            file.display()
        ));
    }
    // Canonicalize: the evaluator's file-path tracking mis-attributes spans
    // for non-canonical paths (e.g. `bootstrap/../tests/foo.tg`), and both
    // sides must agree on the path for the comparison to be meaningful.
    let file = &file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let self_exe = std::env::current_exe().map_err(|e| format!("cannot locate own binary: {e}"))?;
    let tmp = tempfile::TempDir::new().map_err(|e| format!("cannot create temp dir: {e}"))?;
    let bin = tmp.path().join("prog");

    // ── Compile (skipped when the native step is fully overridden) ──────
    let compile_argv: Option<Vec<String>> = match (&overrides.compile, &overrides.native) {
        (Some(argv), _) => Some(argv.clone()),
        (None, Some(_)) => None,
        (None, None) => Some(vec![
            self_exe.display().to_string(),
            "compile".into(),
            file.display().to_string(),
            "-o".into(),
            bin.display().to_string(),
        ]),
    };
    if let Some(argv) = compile_argv {
        let rec = run_with_timeout(&argv, timeout, false)?;
        match rec.status {
            ExecStatus::Timeout => {
                eprintln!("✗ compile timed out after {}s", timeout.as_secs());
                return Ok(Outcome::Timeout);
            }
            ExecStatus::RuntimeError => {
                eprintln!(
                    "✗ compile error — neither side ran:\n{}{}",
                    rec.stdout, rec.stderr
                );
                return Ok(Outcome::CompileError);
            }
            ExecStatus::Ok => {}
        }
    }

    // ── Run both sides ───────────────────────────────────────────────────
    let native_argv = overrides
        .native
        .clone()
        .unwrap_or_else(|| vec![bin.display().to_string()]);
    let native = run_with_timeout(&native_argv, timeout, true)?;

    let eval_argv = overrides.eval.clone().unwrap_or_else(|| {
        vec![
            self_exe.display().to_string(),
            "run".into(),
            file.display().to_string(),
        ]
    });
    let eval = run_with_timeout(&eval_argv, timeout, false)?;

    let outcome = classify(&native, &eval);
    print!("{}", render_report(outcome, &native, &eval, timeout));
    Ok(outcome)
}

/// Render the human-readable outcome report (pure + unit-testable).
/// Writes to `String` are infallible, so `writeln!` results are ignored.
pub(crate) fn render_report(
    outcome: Outcome,
    native: &ExecRecord,
    eval: &ExecRecord,
    timeout: Duration,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    match outcome {
        Outcome::Parity => {
            out.push_str("✓ parity: evaluator and native outputs agree\n");
        }
        Outcome::Divergence => {
            let _ = writeln!(
                out,
                "✗ output divergence (first differing line: {})",
                first_divergence_line(&native.stdout, &eval.stdout)
            );
            push_both(&mut out, native, eval);
        }
        Outcome::NativeRuntimeError => {
            out.push_str("✗ native runtime error with evaluator Ok — itself a finding\n");
            push_both(&mut out, native, eval);
        }
        Outcome::EvalRuntimeError => {
            out.push_str("✗ evaluator error with native Ok\n");
            push_both(&mut out, native, eval);
        }
        Outcome::Timeout => {
            let _ = writeln!(
                out,
                "✗ timeout after {}s on at least one side",
                timeout.as_secs()
            );
            push_both(&mut out, native, eval);
        }
        Outcome::CompileError => {}
    }
    out
}

fn push_both(out: &mut String, native: &ExecRecord, eval: &ExecRecord) {
    use std::fmt::Write as _;
    let _ = writeln!(out, "--- evaluator stdout ({:?}) ---", eval.status);
    let _ = writeln!(out, "{}", eval.stdout);
    let _ = writeln!(out, "--- native stdout ({:?}) ---", native.status);
    let _ = writeln!(out, "{}", native.stdout);
    if !eval.stderr.is_empty() {
        out.push_str("--- evaluator stderr (not compared) ---\n");
        let _ = writeln!(out, "{}", eval.stderr);
    }
    if !native.stderr.is_empty() {
        out.push_str("--- native stderr (not compared) ---\n");
        let _ = writeln!(out, "{}", native.stderr);
    }
}

/// Run `argv` with empty stdin, capturing stdout/stderr, killing the child
/// at `timeout`. `restrict_env` clears the environment except PATH/TMPDIR —
/// applied to the native binary (the program under test), not the tool
/// subprocesses (§2.2 eligibility).
pub(crate) fn run_with_timeout(
    argv: &[String],
    timeout: Duration,
    restrict_env: bool,
) -> Result<ExecRecord, String> {
    let (prog, args) = argv.split_first().ok_or("empty command")?;
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if restrict_env {
        cmd.env_clear();
        for key in ["PATH", "TMPDIR"] {
            if let Ok(v) = std::env::var(key) {
                cmd.env(key, v);
            }
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to run {prog}: {e}"))?;

    // Drain pipes on threads so a chatty child can't deadlock the poll loop.
    let mut out_pipe = child.stdout.take().ok_or("no stdout pipe")?;
    let mut err_pipe = child.stderr.take().ok_or("no stderr pipe")?;
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut out_pipe, &mut buf).ok();
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut err_pipe, &mut buf).ok();
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(|e| format!("wait failed: {e}"))? {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => {
                child.kill().ok();
                child.wait().ok();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };

    let stdout = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    Ok(ExecRecord {
        status: match status {
            None => ExecStatus::Timeout,
            Some(s) if s.success() => ExecStatus::Ok,
            Some(_) => ExecStatus::RuntimeError,
        },
        stdout,
        stderr,
    })
}
