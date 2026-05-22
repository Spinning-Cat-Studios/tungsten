//! Cache command handlers: clean, clean-all, stats, prune.

use std::process::ExitCode;

/// Clean command: clear the build cache for the current project.
pub fn cmd_clean(verbose: bool) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let mut cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    match cache.clear() {
        Ok(()) => {
            // Also clean elaboration cache (ADR 10.5.26l)
            let _ = cache.clean_elab_cache();
            println!("✓ Cache cleared");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not clear cache: {e}");
            ExitCode::from(3)
        }
    }
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

/// Cache stats command: show cache statistics.
pub fn cmd_cache_stats(verbose: bool, json: bool) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    let stats = match cache.stats() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not get cache stats: {e}");
            return ExitCode::from(3);
        }
    };

    let elab_stats = cache.elab_cache_stats().ok();

    if json {
        let oldest_ms = stats.oldest_accessed.map_or(0, |d| d.as_millis());
        let newest_ms = stats.newest_accessed.map_or(0, |d| d.as_millis());
        let elab_count = elab_stats.as_ref().map_or(0, |e| e.entry_count);
        let elab_bytes = elab_stats.as_ref().map_or(0, |e| e.size_bytes);
        println!(
            r#"{{"size_bytes":{},"entry_count":{},"max_size_mb":{},"oldest_accessed_ms":{},"newest_accessed_ms":{},"elab_entry_count":{},"elab_size_bytes":{}}}"#,
            stats.size_bytes,
            stats.entry_count,
            stats.max_size_mb,
            oldest_ms,
            newest_ms,
            elab_count,
            elab_bytes
        );
    } else {
        let size_kb = stats.size_bytes / 1024;
        let size_mb = stats.size_bytes / (1024 * 1024);

        println!("Cache Statistics:");
        println!("  AST entries:  {}", stats.entry_count);
        if size_mb > 0 {
            println!("  AST size:     {size_mb} MB ({size_kb} KB)");
        } else {
            println!("  AST size:     {size_kb} KB");
        }
        println!("  Max size:     {} MB", stats.max_size_mb);

        if let Some(elab) = &elab_stats {
            let elab_kb = elab.size_bytes / 1024;
            println!("  Elab entries: {}", elab.entry_count);
            println!("  Elab size:    {elab_kb} KB");
            if elab.full_output_count > 0 {
                let full_kb = elab.full_output_bytes / 1024;
                let avg_kb = full_kb / elab.full_output_count as u64;
                let compressed = if cfg!(feature = "compress") {
                    " (zstd)"
                } else {
                    ""
                };
                println!(
                    "  Full-output:  {} entries ({full_kb} KB, avg {avg_kb} KB/entry{compressed})",
                    elab.full_output_count
                );
            }
        }

        if let Some(oldest) = stats.oldest_accessed {
            println!("  Oldest:       {} ago", format_duration_ago(oldest));
        }
        if let Some(newest) = stats.newest_accessed {
            println!("  Newest:       {} ago", format_duration_ago(newest));
        }
    }

    ExitCode::SUCCESS
}

/// Cache prune command: remove least recently used entries.
pub fn cmd_cache_prune(verbose: bool, target_mb: Option<u64>) -> ExitCode {
    use std::env;
    use tungsten_bootstrap::cache::BuildCache;

    let cwd = match env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: could not get current directory: {e}");
            return ExitCode::from(3);
        }
    };

    let mut cache = match BuildCache::new(&cwd, verbose) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not open cache: {e}");
            return ExitCode::from(3);
        }
    };

    match cache.prune(target_mb) {
        Ok(stats) => {
            if stats.removed_count == 0 {
                println!(
                    "✓ Cache already within limits ({} KB)",
                    stats.new_size_bytes / 1024
                );
            } else {
                println!(
                    "✓ Pruned {} entries, freed {} KB (new size: {} KB)",
                    stats.removed_count,
                    stats.freed_bytes / 1024,
                    stats.new_size_bytes / 1024
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not prune cache: {e}");
            ExitCode::from(3)
        }
    }
}

/// Format a duration as a human-readable "X ago" string.
fn format_duration_ago(timestamp: std::time::Duration) -> String {
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
