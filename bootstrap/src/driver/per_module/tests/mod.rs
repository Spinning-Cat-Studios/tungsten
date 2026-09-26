mod admission_boundary;
mod fresh_encodings;
mod parallel;
mod signature_collection;
mod stubs;
mod walk;

use super::stubs::*;
use super::*;
use crate::ast::Item;
use crate::driver::output::TraceOptions;
use crate::driver::{pipeline, prepare_project};
use crate::elaborate::{Constructor, ElabError, ModuleExports, TypeDef, TypeDefKind, ValueDef};
use tungsten_core::Type;

fn make_parsed_module(items: Vec<Item>) -> ParsedModule {
    ParsedModule {
        path: std::path::PathBuf::from("test.tg"),
        source_file: crate::ast::SourceFile {
            items,
            span: crate::span::Span::new(0, 0),
        },
        submodules: vec![],
        visibility: crate::ast::Visibility::Public,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Shared on-disk project fixtures (ADR 14.8.26g). Declared here rather than
// in one test file so the walk tests and the cache-gate tests use the SAME
// elaboration entry point — a second copy would let the two drift into
// testing different pipelines under the same names.
// ─────────────────────────────────────────────────────────────────────────

/// Run `elaborate_module_tree` over a throwaway on-disk project (the first
/// entry is the root module) with the **serial** walker, returning the
/// accumulated error list — empty for a clean run.
fn elaborate_tree_errors(files: &[(&str, &str)]) -> Vec<ElabError> {
    elaborate_tree_errors_with_threads(files, 1)
}

/// As [`elaborate_tree_errors`], with `TUNGSTEN_ELAB_THREADS` pinned to
/// `threads` for the duration.
///
/// **Every** fixture call goes through here and takes the lock, not just the
/// ones that want parallelism. The variable is process-global and the walker's
/// scheduling depends on it, so a test that merely *assumes* serial
/// scheduling is exactly as dependent on it as one that changes it — which is
/// how `the_module_bail_out_stops_at_the_display_budget` flaked one run in
/// five: a concurrent parallel test flipped the variable under it, the walk
/// took the per-worker accumulator path, and the bail-out landed at a
/// different module count. Routing the env through the fixture means no test
/// sets it directly and the discipline cannot be half-applied again.
fn elaborate_tree_errors_with_threads(files: &[(&str, &str)], threads: usize) -> Vec<ElabError> {
    let _guard = super::cache::equivalence::ELAB_THREADS_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("TUNGSTEN_ELAB_THREADS", threads.to_string());

    let dir = tempfile::tempdir().unwrap();
    for (name, src) in files {
        std::fs::write(dir.path().join(name), src).unwrap();
    }
    let errors = elaborate_tree_at(dir.path(), files[0].0, None)
        .err()
        .unwrap_or_default();

    std::env::remove_var("TUNGSTEN_ELAB_THREADS");
    errors
}

/// Like [`elaborate_tree_errors`], but writes into a caller-owned `dir` with
/// the elaboration cache live, so the caller can inspect `.tungsten/`
/// afterwards.
fn elaborate_tree_with_cache(
    dir: &std::path::Path,
    files: &[(&str, &str)],
) -> Result<(), Vec<ElabError>> {
    for (name, src) in files {
        std::fs::write(dir.join(name), src).unwrap();
    }
    let cache = std::sync::Mutex::new(crate::cache::BuildCache::new(dir, false).unwrap());
    elaborate_tree_at(dir, files[0].0, Some(&cache))
}

/// The one elaboration entry point both fixtures above go through.
fn elaborate_tree_at(
    dir: &std::path::Path,
    root: &str,
    cache: Option<&std::sync::Mutex<crate::cache::BuildCache>>,
) -> Result<(), Vec<ElabError>> {
    let main_path = dir.join(root);
    let prepared = prepare_project(&main_path, false, cache).expect("fixture must parse");
    let build = pipeline::BuildCtx {
        cache,
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };
    elaborate_module_tree(
        &prepared.module_tree,
        &main_path,
        false,
        &build,
        &TraceOptions::default(),
    )
    .map(|_| ())
}

/// How many elaboration-cache entries exist under `dir`'s `.tungsten/`.
///
/// Counts the `cache/elab` tier only: parse-cache entries (`cache/modules`)
/// are keyed by file content alone and carry no elaboration state, so a
/// failing run writing those leaks nothing — D5 governs elaboration entries.
fn elab_cache_entry_count(dir: &std::path::Path) -> usize {
    match std::fs::read_dir(dir.join(".tungsten").join("cache").join("elab")) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    }
}

/// The file names (without directory) that carry at least one error.
fn failing_files(errors: &[ElabError]) -> Vec<String> {
    let mut files: Vec<String> = errors
        .iter()
        .filter_map(|e| e.file_path.as_ref())
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    files.dedup();
    files
}
