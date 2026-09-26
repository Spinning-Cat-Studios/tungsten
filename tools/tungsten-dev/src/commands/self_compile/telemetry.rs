//! Per-stage max-RSS telemetry (ADR 2.7.26b T4c).
//!
//! Each self-compile stage is run through [`run_with_rusage`], which reaps the
//! child with `wait4(2)` to obtain its `ru_maxrss` (including waited
//! descendants — rusage propagates up wait chains, so the `sh -c 'xargs llc'`
//! pipeline is measured too). Values are appended to
//! `<log_dir>/<cmd>.rss.log`; a telemetry write failure WARNS and never aborts
//! the self-compile. This feeds ADR 2.7.26a's P0 measurement.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

/// Result of running a stage command with resource accounting.
pub struct RunOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Peak resident set size in KiB (child + waited descendants).
    pub max_rss_kb: u64,
}

/// Run `cmd` to completion, capturing output and the child's max-RSS.
pub fn run_with_rusage(cmd: &mut Command) -> std::io::Result<RunOutput> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;

    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let out_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut v);
        v
    });
    let err_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut v);
        v
    });

    let (status, max_rss_kb) = wait_with_rusage(&mut child)?;
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();

    Ok(RunOutput {
        status,
        stdout,
        stderr,
        max_rss_kb,
    })
}

/// Reap the child via `wait4`, returning its exit status and max-RSS (KiB).
/// Falls back to a plain `wait()` (rss = 0) if `wait4` fails.
fn wait_with_rusage(child: &mut std::process::Child) -> std::io::Result<(ExitStatus, u64)> {
    use std::os::unix::process::ExitStatusExt;

    let pid = child.id() as libc::pid_t;
    let mut status_raw: libc::c_int = 0;
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::wait4(pid, &mut status_raw, 0, &mut ru) };
    if rc == pid {
        // ru_maxrss is KiB on Linux, bytes on macOS.
        let raw = ru.ru_maxrss.max(0) as u64;
        let kb = if cfg!(target_os = "macos") {
            raw / 1024
        } else {
            raw
        };
        Ok((ExitStatus::from_raw(status_raw), kb))
    } else {
        Ok((child.wait()?, 0))
    }
}

/// Appends `stage=<name> max_rss_kb=<n>` lines to `<dir>/<cmd>.rss.log`.
/// The file is truncated once per run (matching the "each run overwrites the
/// previous log" convention of the other captured logs).
pub struct RssLogger {
    path: Option<PathBuf>,
}

impl RssLogger {
    /// Open (truncate) the log in `primary`, falling back to `fallback` when
    /// the primary is unwritable. Both failing WARNS and disables logging —
    /// telemetry must never abort a self-compile.
    pub fn create(cmd_name: &str, primary: &Path, fallback: &Path) -> Self {
        let file_name = format!("{cmd_name}.rss.log");
        for dir in [primary, fallback] {
            let path = dir.join(&file_name);
            if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&path, "").is_ok() {
                return Self { path: Some(path) };
            }
        }
        eprintln!(
            "⚠ RSS telemetry disabled: neither {} nor {} is writable",
            primary.display(),
            fallback.display()
        );
        Self { path: None }
    }

    /// Append one stage entry; a write failure warns and continues.
    pub fn log_stage(&self, stage: &str, max_rss_kb: u64) {
        let Some(path) = &self.path else { return };
        let line = format!("stage={stage} max_rss_kb={max_rss_kb}\n");
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
        if let Err(e) = appended {
            eprintln!("⚠ RSS telemetry write failed ({}): {e}", path.display());
        } else {
            eprintln!("  [rss] {stage}: {max_rss_kb} KiB peak");
        }
    }

    #[cfg(test)]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_with_rusage_reports_status_and_rss() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 0"]);
        let out = run_with_rusage(&mut cmd).unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
        assert!(
            out.max_rss_kb > 0,
            "expected nonzero max-RSS for a real child"
        );
    }

    #[test]
    fn run_with_rusage_reports_failure_exit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exit 7"]);
        let out = run_with_rusage(&mut cmd).unwrap();
        assert_eq!(out.status.code(), Some(7));
    }

    #[test]
    fn logger_writes_stage_lines() {
        let dir = tempfile::tempdir().unwrap();
        let logger = RssLogger::create("self-compile-fast", dir.path(), dir.path());
        logger.log_stage("emit-ir", 12345);
        logger.log_stage("link", 42);

        let content = std::fs::read_to_string(logger.path().unwrap()).unwrap();
        assert!(content.contains("stage=emit-ir max_rss_kb=12345"));
        assert!(content.contains("stage=link max_rss_kb=42"));
    }

    /// T4c: unwritable primary falls back to the workspace-local dir.
    #[test]
    fn logger_falls_back_when_primary_unwritable() {
        let dir = tempfile::tempdir().unwrap();
        let primary = Path::new("/nonexistent-tungsten-telemetry/deep");
        let logger = RssLogger::create("self-compile-fast", primary, dir.path());
        let path = logger.path().expect("fallback path");
        assert!(path.starts_with(dir.path()));
        logger.log_stage("emit-ir", 1);
        assert!(std::fs::read_to_string(path).unwrap().contains("emit-ir"));
    }

    /// T4c: both dirs unwritable → warns, never panics/aborts.
    #[test]
    fn logger_is_non_fatal_when_all_unwritable() {
        let bad_a = Path::new("/nonexistent-tungsten-telemetry/a");
        let bad_b = Path::new("/nonexistent-tungsten-telemetry/b");
        let logger = RssLogger::create("self-compile-fast", bad_a, bad_b);
        assert!(logger.path().is_none());
        logger.log_stage("emit-ir", 1); // must not panic
    }

    #[test]
    fn logger_truncates_per_run() {
        let dir = tempfile::tempdir().unwrap();
        let first = RssLogger::create("self-compile-fast", dir.path(), dir.path());
        first.log_stage("emit-ir", 999);
        let second = RssLogger::create("self-compile-fast", dir.path(), dir.path());
        second.log_stage("link", 1);

        let content = std::fs::read_to_string(second.path().unwrap()).unwrap();
        assert!(!content.contains("999"), "previous run should be truncated");
        assert!(content.contains("stage=link"));
    }
}
