//! Compiler invocation for the golden runner. Split out of `main.rs` (ADR
//! 16.7.26b file-size paydown). Runs the tungsten binary on a `.tg` file,
//! merges stderr+stdout the way the `.expected` snapshots capture it, and
//! normalizes the result (ANSI-stripped, paths relativized).

use std::path::Path;
use std::process::Command;

use crate::compare::{normalize_paths, strip_ansi};

pub(crate) fn run_compiler(compiler: &Path, cmd: &str, tg: &Path, extra_args: &[String]) -> String {
    let mut args = vec![cmd.to_string(), tg.to_string_lossy().to_string()];
    args.extend(extra_args.iter().cloned());

    let output = Command::new(compiler).args(&args).output();

    let raw = match output {
        Ok(o) => {
            // stderr first (warnings/errors), then stdout (results)
            // This matches shell `2>&1` ordering for typical compiler output.
            let stderr = String::from_utf8_lossy(&o.stderr);
            let stdout = String::from_utf8_lossy(&o.stdout);
            let mut s = String::new();
            if !stderr.is_empty() {
                s.push_str(stderr.trim_end());
            }
            if !stdout.is_empty() {
                if !s.is_empty() {
                    s.push('\n');
                }
                s.push_str(stdout.trim_end());
            }
            s
        }
        Err(e) => format!("ERROR: failed to run compiler: {e}"),
    };

    let stripped = strip_ansi(&raw);
    normalize_paths(&stripped)
}

pub(crate) fn read_args_file(tg: &Path) -> Vec<String> {
    let args_file = tg.with_extension("args");
    if !args_file.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&args_file)
        .unwrap_or_default()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}
