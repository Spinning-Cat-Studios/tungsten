//! `tungsten-dev selfcompiled-profile` — one-command ADR 2.7.26a §3.4 heap profile.
//!
//! Orchestrates the previously script-scattered methodology (ADR 11.7.26a):
//! build an `--alloc-profile` tungsten1 (fast tier), run
//! `tungsten1 check src/compiler/main.tg` with interim dumps enabled, sample
//! its RSS once per second, kill it at an RSS cap instead of letting it
//! thrash against the container ceiling, and print the per-module delta
//! table (`summarize`). Re-run after every arena/leak fix step — ADR
//! 2.7.26a §4 requires a re-measure between the types and terms stages.

pub mod summarize;

use super::self_compile::{
    self,
    artifacts::{check_executable_format, Artifacts},
    SelfCompileArgs,
};
use clap::Args;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Args)]
pub struct SelfcompiledProfileArgs {
    /// Interim-dump interval in MB of cumulative allocation
    /// (TUNGSTEN_ALLOC_PROFILE_INTERVAL_MB).
    #[arg(long, default_value_t = 1024)]
    pub interval_mb: u64,

    /// Kill the check once its RSS exceeds this many GiB. Near the container
    /// ceiling the run degrades to reclaim-thrash (~1 module / 5 min), and
    /// runs that park near the VM limit can wedge the Docker daemon minutes
    /// later (delayed OOM aftermath) — the default keeps ≥7 GiB of VM
    /// headroom. Raise to ~28 only for deliberate near-ceiling captures.
    #[arg(long, default_value_t = 24.0)]
    pub max_rss_gb: f64,

    /// Reuse the existing ./tungsten1 instead of rebuilding. It must already
    /// be an --alloc-profile build or the log will contain no markers.
    #[arg(long)]
    pub skip_build: bool,

    /// Only summarize an existing stderr log; no build, no run.
    #[arg(long, value_name = "LOG", conflicts_with_all = ["skip_build"])]
    pub summarize: Option<PathBuf>,

    /// Rows in the per-module delta table.
    #[arg(long, default_value_t = 10)]
    pub top: usize,

    /// Add per-module `dMu/dEnv/dRef/dStr` columns and a terminal per-class
    /// share table — attributes the RSS ramp to an allocation class
    /// (ADR 24.7.26a; the "which class drives it?" question of 23.7.26d).
    #[arg(long)]
    pub by_class: bool,

    /// Report the per-module-index delta trend (walk-order first/last-window
    /// means + a SUPER-LINEAR/LINEAR/FLAT verdict) — the "is it O(M) or O(M²)?"
    /// question of ADR 23.7.26d.
    #[arg(long)]
    pub growth: bool,
}

/// How the profiled check ended.
enum RunOutcome {
    Exited(i32),
    KilledAtRssCap { peak_gb: f64 },
    Signalled,
}

pub fn run(args: SelfcompiledProfileArgs) -> Result<(), String> {
    if let Some(log) = &args.summarize {
        return summarize_file(log, args.top, args.by_class, args.growth);
    }

    let arts = Artifacts::resolve()?;
    if !args.skip_build {
        eprintln!("=== selfcompiled-profile: building --alloc-profile tungsten1 (fast tier) ===");
        self_compile::run(SelfCompileArgs {
            fast: true,
            opt: false,
            direct: false,
            x86: false,
            parallelism: None,
            alloc_profile: true,
        })?;
    }
    check_executable_format(&arts.output_binary)?;

    let log_dir = writable_log_dir(&arts)?;
    let stderr_path = log_dir.join("selfcompiled-profile.stderr.log");
    let rss_path = log_dir.join("selfcompiled-profile.rss.log");

    eprintln!(
        "\n=== selfcompiled-profile: {} check {} (interval={}MB, rss-cap={}GiB) ===",
        arts.output_binary.display(),
        arts.entry.display(),
        args.interval_mb,
        args.max_rss_gb,
    );
    let mut child = spawn_profiled_check(&arts, args.interval_mb, &stderr_path, &log_dir)?;
    let outcome = sample_until_exit(&mut child, args.max_rss_gb, &rss_path)?;

    let stderr_text = fs::read_to_string(&stderr_path)
        .map_err(|e| format!("cannot read {}: {e}", stderr_path.display()))?;
    let snapshots = summarize::parse_profile_log(&stderr_text);
    print!(
        "\n{}",
        summarize::render_summary(&snapshots, args.top, args.by_class, args.growth)
    );
    match outcome {
        RunOutcome::Exited(code) => eprintln!("outcome: check exited {code}"),
        RunOutcome::KilledAtRssCap { peak_gb } => eprintln!(
            "outcome: KILLED at the {peak_gb:.1} GiB RSS cap (reclaim-thrash \
             territory — the profile above is the terminal state)"
        ),
        RunOutcome::Signalled => eprintln!("outcome: check died to a signal (OOM kill?)"),
    }
    eprintln!("logs: {} + {}", stderr_path.display(), rss_path.display());
    Ok(())
}

fn summarize_file(log: &Path, top: usize, by_class: bool, growth: bool) -> Result<(), String> {
    let text =
        fs::read_to_string(log).map_err(|e| format!("cannot read {}: {e}", log.display()))?;
    let snapshots = summarize::parse_profile_log(&text);
    if snapshots.is_empty() {
        return Err(format!(
            "{} contains no [alloc-profile] markers — was the binary built with --alloc-profile?",
            log.display()
        ));
    }
    print!(
        "{}",
        summarize::render_summary(&snapshots, top, by_class, growth)
    );
    Ok(())
}

/// Prefer the bind-mounted log dir; fall back to `.devcontainer/logs`.
fn writable_log_dir(arts: &Artifacts) -> Result<PathBuf, String> {
    if arts.log_dir.is_dir() {
        return Ok(arts.log_dir.clone());
    }
    fs::create_dir_all(&arts.fallback_log_dir)
        .map_err(|e| format!("cannot create {}: {e}", arts.fallback_log_dir.display()))?;
    Ok(arts.fallback_log_dir.clone())
}

fn spawn_profiled_check(
    arts: &Artifacts,
    interval_mb: u64,
    stderr_path: &Path,
    log_dir: &Path,
) -> Result<Child, String> {
    let stderr_file = fs::File::create(stderr_path)
        .map_err(|e| format!("cannot create {}: {e}", stderr_path.display()))?;
    let stdout_path = log_dir.join("selfcompiled-profile.stdout.log");
    let stdout_file = fs::File::create(&stdout_path)
        .map_err(|e| format!("cannot create {}: {e}", stdout_path.display()))?;
    Command::new(&arts.output_binary)
        .arg("check")
        .arg(&arts.entry)
        .current_dir(&arts.root)
        .env(
            "TUNGSTEN_ALLOC_PROFILE_INTERVAL_MB",
            interval_mb.to_string(),
        )
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("cannot spawn {}: {e}", arts.output_binary.display()))
}

/// 1 Hz VmRSS sampler: logs `t=<epoch> rss_kb=<n>`, kills the child once it
/// crosses the cap. Off-Linux (`/proc` absent) it degrades to a plain wait.
fn sample_until_exit(
    child: &mut Child,
    max_rss_gb: f64,
    rss_path: &Path,
) -> Result<RunOutcome, String> {
    let cap_kb = (max_rss_gb * 1024.0 * 1024.0) as u64;
    let mut rss_log = fs::File::create(rss_path)
        .map_err(|e| format!("cannot create {}: {e}", rss_path.display()))?;
    let mut peak_kb: u64 = 0;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(match status.code() {
                Some(code) => RunOutcome::Exited(code),
                None => RunOutcome::Signalled,
            });
        }
        if let Some(rss_kb) = vm_rss_kb_of(child.id()) {
            peak_kb = peak_kb.max(rss_kb);
            let _ = writeln!(rss_log, "t={} rss_kb={rss_kb}", epoch_secs());
            if rss_kb > cap_kb {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(RunOutcome::KilledAtRssCap {
                    peak_gb: peak_kb as f64 / (1024.0 * 1024.0),
                });
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// VmRSS of `pid` in KiB (Linux; `None` elsewhere).
fn vm_rss_kb_of(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_mode_rejects_marker_free_logs() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("empty.log");
        fs::write(&log, "no markers here\n").unwrap();
        let err = summarize_file(&log, 5, false, false).unwrap_err();
        assert!(err.contains("--alloc-profile"), "{err}");
    }

    #[test]
    fn rss_cap_converts_to_kb() {
        // 28 GiB cap → 29,360,128 KiB; sanity-check the arithmetic used in
        // sample_until_exit so a units slip can't silently disable the cap.
        let cap_kb = (28.0_f64 * 1024.0 * 1024.0) as u64;
        assert_eq!(cap_kb, 29_360_128);
    }
}
