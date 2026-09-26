//! `tungsten cache status` / `cache stats` — aggregate cache counts, and the
//! root they were counted in (ADR 5.8.26d D5).
//!
//! The rendering is split out as **pure functions returning values** rather than
//! `println!`s for the reason ADR 7.7.26j's gates exist: a printing function is
//! executed by any test that calls it and asserted by none. Measured on the
//! first draft of this command — 49.3% diff coverage and 17 surviving mutants,
//! every one of them an arithmetic or comparison operator inside a `println!`
//! argument (`/`→`*` in the KB conversion, `>`→`<` on the size branches). The
//! formatters below are the same code with a return type, and the tests assert
//! the strings.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::cache::{BuildCache, CacheStats, ElabCacheStats};

use super::cache_outcome::CacheOutcome;
use super::cache_root::{self, CacheRoot};

/// Render the statistics as a single JSON object.
///
/// `root` leads the object for the same reason it leads the human rendering: a
/// count is not interpretable without the root it counts.
pub(crate) fn format_stats_json(
    root: &Path,
    stats: &CacheStats,
    elab_stats: Option<&ElabCacheStats>,
) -> String {
    let oldest_ms = stats.oldest_accessed.map_or(0, |d| d.as_millis());
    let newest_ms = stats.newest_accessed.map_or(0, |d| d.as_millis());
    let elab_count = elab_stats.map_or(0, |e| e.entry_count);
    let elab_bytes = elab_stats.map_or(0, |e| e.size_bytes);
    format!(
        r#"{{"root":"{}","size_bytes":{},"entry_count":{},"max_size_mb":{},"oldest_accessed_ms":{},"newest_accessed_ms":{},"elab_entry_count":{},"elab_size_bytes":{}}}"#,
        root.display(),
        stats.size_bytes,
        stats.entry_count,
        stats.max_size_mb,
        oldest_ms,
        newest_ms,
        elab_count,
        elab_bytes
    )
}

/// Render the statistics in the human-readable form, root first.
pub(crate) fn format_stats_human(
    root: &CacheRoot,
    file: Option<&Path>,
    stats: &CacheStats,
    elab_stats: Option<&ElabCacheStats>,
) -> Vec<String> {
    let size_kb = stats.size_bytes / 1024;
    let size_mb = stats.size_bytes / (1024 * 1024);

    let mut lines = vec![
        "Cache Statistics:".to_string(),
        format!(
            "  Root:         {} {}",
            root.path.display(),
            root.provenance(file)
        ),
        format!("  AST entries:  {}", stats.entry_count),
    ];
    if size_mb > 0 {
        lines.push(format!("  AST size:     {size_mb} MB ({size_kb} KB)"));
    } else {
        lines.push(format!("  AST size:     {size_kb} KB"));
    }
    lines.push(format!("  Max size:     {} MB", stats.max_size_mb));

    if let Some(elab) = elab_stats {
        let elab_kb = elab.size_bytes / 1024;
        lines.push(format!("  Elab entries: {}", elab.entry_count));
        lines.push(format!("  Elab size:    {elab_kb} KB"));
        if elab.full_output_count > 0 {
            let full_kb = elab.full_output_bytes / 1024;
            let avg_kb = full_kb / elab.full_output_count as u64;
            let compressed = if cfg!(feature = "compress") {
                " (zstd)"
            } else {
                ""
            };
            lines.push(format!(
                "  Full-output:  {} entries ({full_kb} KB, avg {avg_kb} KB/entry{compressed})",
                elab.full_output_count
            ));
        }
    }

    if let Some(oldest) = stats.oldest_accessed {
        lines.push(format!(
            "  Oldest:       {} ago",
            super::cache::format_duration_ago(oldest)
        ));
    }
    if let Some(newest) = stats.newest_accessed {
        lines.push(format!(
            "  Newest:       {} ago",
            super::cache::format_duration_ago(newest)
        ));
    }
    lines
}

/// Gather and render cache statistics for `file`'s project (or `cwd`).
///
/// Returns the outcome as a value; `cmd_cache_stats` is the thin shell that
/// prints it and turns it into an exit code.
pub(crate) fn run_cache_stats(
    verbose: bool,
    json: bool,
    file: Option<&Path>,
    cwd: &Path,
) -> CacheOutcome {
    if let Some(msg) = cache_root::operand_error(file) {
        return CacheOutcome::Failed(msg);
    }
    let root = cache_root::resolve(file, cwd);

    let cache = match BuildCache::new(&root.path, verbose) {
        Ok(c) => c,
        Err(e) => return CacheOutcome::Failed(format!("could not open cache: {e}")),
    };
    let stats = match cache.stats() {
        Ok(s) => s,
        Err(e) => return CacheOutcome::Failed(format!("could not get cache stats: {e}")),
    };
    let elab_stats = cache.elab_cache_stats().ok();

    let lines = if json {
        vec![format_stats_json(&root.path, &stats, elab_stats.as_ref())]
    } else {
        format_stats_human(&root, file, &stats, elab_stats.as_ref())
    };
    CacheOutcome::Reported(lines)
}

/// Cache stats command: show cache statistics.
///
/// `file` is the optional entry-file operand (ADR 5.8.26d D5): with it, the root
/// is resolved the way the WRITER resolves it — from the entry file's parent —
/// instead of from the current working directory. The root is reported either
/// way, so a count is never readable without the root it counts.
pub fn cmd_cache_stats(verbose: bool, json: bool, file: Option<&Path>) -> ExitCode {
    let cwd: PathBuf = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    run_cache_stats(verbose, json, file, &cwd).report()
}

// Tests: cache_stats_tests.rs
#[cfg(test)]
#[path = "cache_stats_tests.rs"]
mod cache_stats_tests;
