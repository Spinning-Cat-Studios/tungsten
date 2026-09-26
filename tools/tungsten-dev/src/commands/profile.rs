//! `tungsten-dev profile` — Build with tracing and capture a Chrome trace (ADR 10.5.26j §2.4).

use clap::Args;
use std::process::Command;

const TUNGSTEN_BIN: &str = "./target/release/tungsten";
const PROFILE_DIR: &str = "/var/log/tungsten/profiles";
const ENTRY_FILE: &str = "src/compiler/main.tg";
const OUTPUT_BINARY: &str = "tungsten1";

#[derive(Args)]
pub struct ProfileArgs {
    /// Number of codegen jobs (defaults to nproc)
    #[arg(short, long)]
    pub jobs: Option<usize>,

    /// Output path for the trace file
    #[arg(short, long)]
    pub output: Option<String>,
}

pub fn run(args: ProfileArgs) -> Result<(), String> {
    let jobs = args.jobs.unwrap_or_else(num_cpus);

    // Step 1: Build the compiler with codegen + profile features
    eprintln!("=== Stage 1/2: Building with --features codegen,profile ===");
    build_with_profile()?;

    // Step 2: Run the compile with tracing enabled
    eprintln!("\n=== Stage 2/2: Profiling self-compile ({jobs} job(s)) ===");
    std::fs::create_dir_all(PROFILE_DIR)
        .map_err(|e| format!("failed to create {PROFILE_DIR}: {e}"))?;

    let trace_path = args.output.unwrap_or_else(|| {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("{PROFILE_DIR}/trace-{timestamp}.json")
    });

    run_profiled_compile(&trace_path, jobs)?;

    eprintln!("\n✓ Trace written to {trace_path}");
    eprintln!("  Open in https://ui.perfetto.dev");
    Ok(())
}

fn build_with_profile() -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "tungsten_bootstrap",
            "--features",
            "codegen,profile",
        ])
        .status()
        .map_err(|e| format!("failed to run cargo build: {e}"))?;
    if !status.success() {
        return Err("cargo build --features codegen,profile failed".to_string());
    }
    Ok(())
}

fn run_profiled_compile(trace_path: &str, jobs: usize) -> Result<(), String> {
    let status = Command::new(TUNGSTEN_BIN)
        .args(["compile", ENTRY_FILE, "-o", OUTPUT_BINARY])
        .env("TUNGSTEN_TRACE_FILE", trace_path)
        .env("TUNGSTEN_CODEGEN_JOBS", jobs.to_string())
        .status()
        .map_err(|e| format!("failed to run tungsten compile: {e}"))?;
    if !status.success() {
        return Err("profiled compile failed".to_string());
    }
    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_jobs() {
        // num_cpus should return something reasonable
        assert!(num_cpus() >= 1);
    }

    #[test]
    fn test_profile_constants() {
        assert!(
            TUNGSTEN_BIN.contains("tungsten"),
            "binary path should reference tungsten"
        );
        assert!(
            PROFILE_DIR.starts_with("/var/log/tungsten"),
            "profile dir should be under /var/log/tungsten"
        );
        assert!(
            ENTRY_FILE.ends_with(".tg"),
            "entry file should be a .tg file"
        );
    }

    #[test]
    fn test_profile_args_defaults() {
        // Default ProfileArgs should have None for both optional fields
        let args = ProfileArgs {
            jobs: None,
            output: None,
        };
        assert!(args.jobs.is_none());
        assert!(args.output.is_none());
    }

    #[test]
    fn test_profile_args_custom_values() {
        let args = ProfileArgs {
            jobs: Some(8),
            output: Some("/tmp/trace.json".to_string()),
        };
        assert_eq!(args.jobs, Some(8));
        assert_eq!(args.output.as_deref(), Some("/tmp/trace.json"));
    }
}
