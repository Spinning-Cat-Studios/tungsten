//! Build cache initialization and management for the compilation pipeline.

use std::path::Path;
use std::sync::Mutex;

use super::modules::extract_module_dependencies;
use super::prepare_project;
use super::PipelineError;
use super::PipelineOpts;
use super::PreparedProject;
use crate::cache::BuildCache;

/// Prepare a project with optional build cache support.
///
/// Handles:
/// - Cache initialization (respecting `--no-cache` and `TUNGSTEN_NO_CACHE`)
/// - Project parsing
/// - Dependency graph update and cache flush
pub(super) fn prepare_project_with_cache(
    path: &Path,
    opts: &PipelineOpts,
    no_cache: bool,
) -> Result<(Option<Mutex<BuildCache>>, PreparedProject), PipelineError> {
    let project_root = path.parent().unwrap_or(Path::new("."));

    // Check if caching is disabled via CLI flag or environment variable
    let cache_disabled_by_env = std::env::var("TUNGSTEN_NO_CACHE")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let skip_cache = no_cache || cache_disabled_by_env;

    // Initialize build cache (unless disabled)
    let cache = initialize_cache(
        project_root,
        skip_cache,
        opts.verbose,
        no_cache,
        cache_disabled_by_env,
    );

    // Parse and prepare the project (parse errors are already reported to stderr)
    let prepared = match prepare_project(path, opts.verbose, cache.as_ref()) {
        Ok(p) => p,
        Err(PipelineError::ElabFailed(ref msg)) if msg == "parse errors" => {
            return Err(PipelineError::ElabFailed("parse errors".to_string()));
        }
        Err(e) => return Err(e),
    };

    // Update dependency graph and flush cache
    if let Some(ref c) = cache {
        let deps = extract_module_dependencies(&prepared.module_tree);
        let root = path.to_path_buf();
        let modules: Vec<_> = deps
            .into_iter()
            .map(|d| (d.path, d.content_hash, d.dependencies))
            .collect();
        let mut guard = c.lock().unwrap();
        guard.update_dependency_graph(root, modules);
        if let Err(e) = guard.flush() {
            if opts.verbose {
                eprintln!("[cache] warning: failed to flush cache: {e}");
            }
        }
    }

    Ok((cache, prepared))
}

/// Initialize the build cache, or return None if caching is disabled.
///
/// Returns an owned `Option<Mutex<BuildCache>>`. Callers that need a
/// `Option<&Mutex<BuildCache>>` (e.g. for `BuildCtx.cache`) should use
/// `.as_ref()` on the result.
#[allow(clippy::fn_params_excessive_bools)] // Reason: config flags are naturally bool
fn initialize_cache(
    project_root: &Path,
    skip_cache: bool,
    verbose: bool,
    no_cache: bool,
    cache_disabled_by_env: bool,
) -> Option<Mutex<BuildCache>> {
    if skip_cache {
        if verbose {
            eprintln!("{}", cache_disabled_reason(no_cache, cache_disabled_by_env));
        }
        None
    } else {
        match BuildCache::new(project_root, verbose) {
            Ok(c) => Some(Mutex::new(c)),
            Err(e) => {
                if verbose {
                    eprintln!("[cache] warning: failed to initialize cache: {e}");
                }
                None
            }
        }
    }
}

/// Return the human-readable reason the cache was disabled.
///
/// Extracted from `initialize_cache` for testability.
pub(crate) fn cache_disabled_reason(no_cache: bool, cache_disabled_by_env: bool) -> &'static str {
    if no_cache && cache_disabled_by_env {
        "[cache] disabled by --no-cache flag and TUNGSTEN_NO_CACHE env var"
    } else if no_cache {
        "[cache] disabled by --no-cache flag"
    } else {
        "[cache] disabled by TUNGSTEN_NO_CACHE env var"
    }
}
