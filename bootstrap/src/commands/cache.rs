//! Cache command handlers: clean, clean-all, prune.
//!
//! `stats`/`status` lives in [`super::cache_stats`], where its rendering is
//! expressed as pure functions so the gates can reach it (ADR 5.8.26d P3).

use std::path::Path;
use std::process::ExitCode;

use tungsten_bootstrap::cache::{BuildCache, PruneStats};

use super::cache_outcome::CacheOutcome;
use super::cache_root;

/// Clear the build cache for `file`'s project (or `cwd`), reporting the root.
///
/// The root is reported for the same reason `cache status` reports it
/// (ADR 5.8.26d D5): the writer resolves it from the ENTRY FILE's parent, so a
/// bare run from the repo root clears the cwd — quite possibly nothing — while
/// the project's cache sits elsewhere. Silently clearing the wrong cache is
/// worse than silently reporting the wrong one, because the user then believes
/// they have a cold tree.
pub(crate) fn run_clean(verbose: bool, file: Option<&Path>, cwd: &Path) -> CacheOutcome {
    if let Some(msg) = cache_root::operand_error(file) {
        return CacheOutcome::Failed(msg);
    }
    let root = cache_root::resolve(file, cwd);

    let mut cache = match BuildCache::new(&root.path, verbose) {
        Ok(c) => c,
        Err(e) => return CacheOutcome::Failed(format!("could not open cache: {e}")),
    };

    match cache.clear() {
        Ok(()) => {
            // Also clean elaboration cache (ADR 10.5.26l)
            let _ = cache.clean_elab_cache();
            CacheOutcome::Reported(vec![format!("✓ Cache cleared ({})", root.path.display())])
        }
        Err(e) => CacheOutcome::Failed(format!("could not clear cache: {e}")),
    }
}

/// Clean command: clear the build cache for a project.
pub fn cmd_clean(verbose: bool, file: Option<&Path>) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };
    run_clean(verbose, file, &cwd).report()
}

/// Recursively find and remove all `.tungsten` directories under the current
/// working directory, skipping anything under `target/`.
///
/// Equivalent to:
/// ```sh
/// find . -path '*/.tungsten' -type d -not -path '*/target/*' -exec rm -rf {} +
/// ```
pub fn cmd_cache_clean_all(verbose: bool, dry_run: bool) -> ExitCode {
    use std::env;
    use std::fs;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let mut found: Vec<std::path::PathBuf> = Vec::new();
    collect_tungsten_dirs(&cwd, &mut found);

    if found.is_empty() {
        println!("No .tungsten cache directories found.");
        return ExitCode::SUCCESS;
    }

    for dir in &found {
        let display = dir.strip_prefix(&cwd).unwrap_or(dir);
        if dry_run {
            println!("  would remove: {}", display.display());
        } else {
            if verbose {
                eprintln!("  removing: {}", display.display());
            }
            if let Err(e) = fs::remove_dir_all(dir) {
                eprintln!("warning: could not remove {}: {e}", display.display());
            }
        }
    }

    if dry_run {
        println!(
            "Dry run: {} .tungsten director{} would be removed.",
            found.len(),
            if found.len() == 1 { "y" } else { "ies" }
        );
    } else {
        println!(
            "✓ All .tungsten caches cleared ({} director{} removed)",
            found.len(),
            if found.len() == 1 { "y" } else { "ies" }
        );
    }
    ExitCode::SUCCESS
}

/// Walk `dir` recursively, collecting paths to `.tungsten` directories.
/// Skips `target/` subtrees entirely. When a `.tungsten` dir is found, it is
/// added to `out` and its subtree is not descended into.
fn collect_tungsten_dirs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Skip target/ subtrees entirely.
        if name == "target" {
            continue;
        }
        if name == ".tungsten" {
            out.push(path);
            // Don't descend into .tungsten — we're removing the whole thing.
            continue;
        }
        collect_tungsten_dirs(&path, out);
    }
}

/// Render a completed prune, root first (ADR 5.8.26d D5).
///
/// Pure so the KB arithmetic and the empty-prune branch are assertable; the
/// same operators inside a `println!` survived mutation testing on `cache status`.
pub(crate) fn format_prune(stats: &PruneStats, root: &Path) -> Vec<String> {
    let line = if stats.removed_count == 0 {
        format!(
            "✓ Cache already within limits ({} KB)",
            stats.new_size_bytes / 1024
        )
    } else {
        format!(
            "✓ Pruned {} entries, freed {} KB (new size: {} KB)",
            stats.removed_count,
            stats.freed_bytes / 1024,
            stats.new_size_bytes / 1024
        )
    };
    vec![line, format!("  Root:         {}", root.display())]
}

/// Prune `file`'s project cache (or `cwd`'s) to a target size.
pub(crate) fn run_cache_prune(
    verbose: bool,
    target_mb: Option<u64>,
    file: Option<&Path>,
    cwd: &Path,
) -> CacheOutcome {
    if let Some(msg) = cache_root::operand_error(file) {
        return CacheOutcome::Failed(msg);
    }
    let root = cache_root::resolve(file, cwd);

    let mut cache = match BuildCache::new(&root.path, verbose) {
        Ok(c) => c,
        Err(e) => return CacheOutcome::Failed(format!("could not open cache: {e}")),
    };

    match cache.prune(target_mb) {
        Ok(stats) => CacheOutcome::Reported(format_prune(&stats, &root.path)),
        Err(e) => CacheOutcome::Failed(format!("could not prune cache: {e}")),
    }
}

/// Cache prune command: remove least recently used entries.
pub fn cmd_cache_prune(verbose: bool, target_mb: Option<u64>, file: Option<&Path>) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };
    run_cache_prune(verbose, target_mb, file, &cwd).report()
}

/// Format a duration as a human-readable "X ago" string.
pub(super) fn format_duration_ago(timestamp: std::time::Duration) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let elapsed = now.saturating_sub(timestamp);
    let secs = elapsed.as_secs();

    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

// Tests: cache_tests.rs
#[cfg(test)]
#[path = "cache_tests.rs"]
mod cache_tests;
