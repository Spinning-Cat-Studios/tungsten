//! `tungsten diff cache` — differential cold-vs-warm cache parity check
//! (ADR 4.7.26d §2.2).
//!
//! Runs a program **cold** (a fresh, isolated cache dir) and then **warm** (a
//! second run reusing that dir) and compares observable outcome — exit status +
//! stdout. The 4.7.26c bug was a silent cold-vs-warm divergence: the first run
//! is correct, the second reads a bodyless signature-cache entry and reports
//! "no tests found" / spurious `E0030`. `diff exec` detects native-vs-evaluator
//! divergence one axis over; this is its cold-vs-warm analog and can sit in CI
//! as a canary for the whole cache-poisoning class.
//!
//! **What is compared:** exit status (Ok/RuntimeError/Timeout) and stdout,
//! byte-for-byte after trailing-newline normalization. For `--mode test` the
//! stdout carries the discovered/passed test summary, so stdout parity subsumes
//! test-count parity.
//!
//! **Exit codes (a subset of `diff exec`'s vocabulary):** 0 parity; 1
//! divergence (the cache-poisoning class; incl. a one-sided runtime error);
//! 3 compile/elaboration error (neither side ran); 5 timeout. `diff exec`'s
//! `2`/`4` runtime-error codes do not apply — cold and warm share one evaluator
//! path, so a runtime error identical on both sides is parity and a one-sided
//! one is divergence.
//!
//! **Isolation:** the cold side always uses a throwaway temp dir (never the
//! project's `.tungsten`), and the warm side reuses that same dir, so the check
//! is hermetic. `TUNGSTEN_ELAB_CACHE=1` is set on both so the cold side actually
//! populates the elab cache the warm side reads.
//!
//! **Testability hook:** `TUNGSTEN_DIFF_CACHE_{COLD,WARM}_OVERRIDE` (whitespace-
//! split argv, test-only) replace the corresponding run, so divergence/timeout
//! classes are testable without running the compiler.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use super::exec::{argv_from_env, normalize, run_with_timeout, ExecRecord, ExecStatus};

#[cfg(test)]
mod tests;

/// Cold-vs-warm outcome classes with their §2.2 exit codes (mutually exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Parity,
    Divergence,
    CompileError,
    Timeout,
}

impl Outcome {
    pub(crate) fn exit_code(self) -> u8 {
        match self {
            Outcome::Parity => 0,
            Outcome::Divergence => 1,
            Outcome::CompileError => 3,
            Outcome::Timeout => 5,
        }
    }
}

/// Classify the cold and warm records into an outcome (pure + unit-testable).
///
/// `CompileError` is short-circuited before both sides run, so it never reaches
/// here. A timeout on either side dominates. A one-sided runtime error is a
/// divergence; when statuses agree (both Ok or both errored) stdout decides.
pub(crate) fn classify(cold: &ExecRecord, warm: &ExecRecord) -> Outcome {
    match (cold.status, warm.status) {
        (ExecStatus::Timeout, _) | (_, ExecStatus::Timeout) => Outcome::Timeout,
        (ExecStatus::Ok, ExecStatus::RuntimeError) | (ExecStatus::RuntimeError, ExecStatus::Ok) => {
            Outcome::Divergence
        }
        _ => {
            if comparable(&cold.stdout) == comparable(&warm.stdout) {
                Outcome::Parity
            } else {
                Outcome::Divergence
            }
        }
    }
}

/// The stdout of a run, reduced to what is actually comparable between a cold
/// and a warm run of the *same* program.
///
/// Beyond the shared newline trim, this elides the test runner's wall-clock
/// summary (`finished in 0.01s`). That duration is **observation noise, not
/// program semantics**: the warm run is faster by construction — that is the
/// entire point of the cache — so comparing it verbatim made
/// `--mode test --gate` report a divergence on timing alone. Measured 3 of 5
/// runs before this (`tests/dead_arm_letelse_run.tg`: `0.01s` cold vs `0.00s`
/// warm), which made the gate unusable for exactly the mode ADR 4.7.26c's bug
/// lived in.
///
/// Deliberately narrow: only the numeric duration is replaced, so the pass /
/// fail / skipped counts on the same line still decide parity. A cache that
/// poisons a test run changes those counts (4.7.26c turned them into "no tests
/// found"), never just the clock.
pub(crate) fn comparable(stdout: &str) -> String {
    let trimmed = normalize(stdout);
    let mut out = String::with_capacity(trimmed.len());
    let mut rest = trimmed;
    while let Some(at) = rest.find(DURATION_PREFIX) {
        out.push_str(&rest[..at + DURATION_PREFIX.len()]);
        out.push_str("<elided>");
        // Skip the duration itself: digits, dots, and the trailing unit.
        let tail = &rest[at + DURATION_PREFIX.len()..];
        let end = tail
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .map_or(tail.len(), |i| i + usize::from(tail[i..].starts_with('s')));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// The test runner's duration marker (`test_runner`'s summary line).
const DURATION_PREFIX: &str = "finished in ";

/// Test-only overrides for the two run steps (ADR 4.7.26d §2.2 hook).
#[derive(Default)]
pub(crate) struct CacheOverrides {
    pub(crate) cold: Option<Vec<String>>,
    pub(crate) warm: Option<Vec<String>>,
}

impl CacheOverrides {
    /// Read the hidden env-var hooks (whitespace-split argv; test-only).
    pub(crate) fn from_env() -> Self {
        CacheOverrides {
            cold: argv_from_env("TUNGSTEN_DIFF_CACHE_COLD_OVERRIDE"),
            warm: argv_from_env("TUNGSTEN_DIFF_CACHE_WARM_OVERRIDE"),
        }
    }
}

/// Entry point for `tungsten diff cache <file> [--mode run|test] [--gate]`.
pub(crate) fn cmd_diff_cache(
    file: &Path,
    mode: &str,
    timeout_secs: u64,
    gate: bool,
    overrides: &CacheOverrides,
) -> ExitCode {
    if mode != "run" && mode != "test" {
        eprintln!("error: invalid --mode '{mode}' (expected run|test)");
        return ExitCode::from(Outcome::CompileError.exit_code());
    }
    let timeout = Duration::from_secs(timeout_secs);
    match run_diff_cache(file, mode, timeout, overrides) {
        Ok(outcome) => {
            // `--gate` only distinguishes a genuine divergence from everything
            // else; today every non-parity outcome already exits non-zero, so
            // the flag is a documented CI affordance, not a behaviour change.
            let _ = gate;
            ExitCode::from(outcome.exit_code())
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(Outcome::CompileError.exit_code())
        }
    }
}

fn run_diff_cache(
    file: &Path,
    mode: &str,
    timeout: Duration,
    overrides: &CacheOverrides,
) -> Result<Outcome, String> {
    if !file.exists() {
        return Err(format!(
            "source file not found: {} (exit 3: neither side ran)",
            file.display()
        ));
    }
    let file = &file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let self_exe = std::env::current_exe().map_err(|e| format!("cannot locate own binary: {e}"))?;

    // ── Compile/elaboration probe (skipped when the cold step is overridden) ──
    // A file that does not elaborate means "neither side ran" → exit 3. Runs in
    // its own throwaway dir so it can't warm the cold side's cache.
    if overrides.cold.is_none() {
        let probe = elaboration_probe(&self_exe, file, timeout)?;
        if probe.status != ExecStatus::Ok {
            eprintln!(
                "✗ compile/elaboration error — neither side ran:\n{}{}",
                probe.stdout, probe.stderr
            );
            return Ok(Outcome::CompileError);
        }
    }

    // ── Cold + warm, sharing one isolated cache dir ──────────────────────────
    let tmp = tempfile::TempDir::new().map_err(|e| format!("cannot create temp dir: {e}"))?;
    let staged = stage_file(file, tmp.path())?;

    let default_argv = |extra_env: bool| {
        let mut argv = Vec::new();
        if extra_env {
            argv.push("env".to_string());
            argv.push("TUNGSTEN_ELAB_CACHE=1".to_string());
        }
        argv.push(self_exe.display().to_string());
        argv.push(mode.to_string());
        argv.push(staged.display().to_string());
        argv
    };

    let cold_argv = overrides.cold.clone().unwrap_or_else(|| default_argv(true));
    let cold = run_with_timeout(&cold_argv, timeout, false)?;

    let warm_argv = overrides.warm.clone().unwrap_or_else(|| default_argv(true));
    let warm = run_with_timeout(&warm_argv, timeout, false)?;

    let outcome = classify(&cold, &warm);
    print!("{}", render_report(outcome, &cold, &warm, timeout));
    Ok(outcome)
}

/// Run `tungsten check <file>` in a throwaway dir to detect elaboration errors.
fn elaboration_probe(
    self_exe: &Path,
    file: &Path,
    timeout: Duration,
) -> Result<ExecRecord, String> {
    let tmp = tempfile::TempDir::new().map_err(|e| format!("cannot create temp dir: {e}"))?;
    let staged = stage_file(file, tmp.path())?;
    let argv = vec![
        self_exe.display().to_string(),
        "check".to_string(),
        staged.display().to_string(),
    ];
    run_with_timeout(&argv, timeout, false)
}

/// Copy `file` into `dir`, returning the staged path.
fn stage_file(file: &Path, dir: &Path) -> Result<std::path::PathBuf, String> {
    let name = file
        .file_name()
        .ok_or_else(|| format!("input has no file name: {}", file.display()))?;
    let staged = dir.join(name);
    std::fs::copy(file, &staged)
        .map_err(|e| format!("cannot stage {} into temp dir: {e}", file.display()))?;
    Ok(staged)
}

/// Render the human-readable outcome report (pure + unit-testable).
pub(crate) fn render_report(
    outcome: Outcome,
    cold: &ExecRecord,
    warm: &ExecRecord,
    timeout: Duration,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "  cold (fresh cache):  exit {}, stdout {:?}",
        status_word(cold.status),
        normalize(&cold.stdout),
    );
    let _ = writeln!(
        out,
        "  warm (reused cache): exit {}, stdout {:?}",
        status_word(warm.status),
        normalize(&warm.stdout),
    );
    match outcome {
        Outcome::Parity => out.push_str("✓ parity: cold and warm runs agree\n"),
        Outcome::Divergence => {
            out.push_str(
                "✗ divergence: warm run disagrees with cold — cache poisoning (ADR 4.7.26c)\n",
            );
        }
        Outcome::Timeout => {
            let _ = writeln!(
                out,
                "✗ timeout after {}s on at least one side",
                timeout.as_secs()
            );
        }
        Outcome::CompileError => {}
    }
    out
}

fn status_word(status: ExecStatus) -> &'static str {
    match status {
        ExecStatus::Ok => "0",
        ExecStatus::RuntimeError => "err",
        ExecStatus::Timeout => "timeout",
    }
}
