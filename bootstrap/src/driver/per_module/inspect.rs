//! Per-module elaboration-cache tier inspection (ADR 4.7.26d).
//!
//! Reproduces the exact cache key the per-module elaboration loop uses —
//! `f(compiler_version, per-module content hash, the single global Signature Collection
//! exports hash)` — so that, for each module, we can report which cache tier is
//! present on disk (full-output / signature-only / uncached) and whether that
//! entry would serve `CoreDef` bodies to a `run`/`test`.
//!
//! This runs Stub Registration + Signature Collection (stub + signature collection) to recompute the
//! global exports hash, but deliberately skips Body Elaboration body elaboration — the
//! expensive part. It is read-only: no cache writes, no new cache format.
//!
//! The signature-only tier (ADR 10.5.26n) carries no bodies, which is exactly
//! the 4.7.26c hazard: a warm `run`/`test` that consumed it would inherit an
//! empty def list. `cache inspect` surfaces that as `serves_bodies = No`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::cache::elab_cache::{self, hex_string};
use crate::cache::{BuildCache, COMPILER_VERSION, FULL_OUTPUT_SCHEMA_VERSION};

use super::accumulator::ModuleTreeAccumulator;
use super::{phases, stubs};
use crate::driver::modules::ParsedModule;
use crate::driver::pipeline::{self, BuildCtx};
use crate::driver::{prepare_project, PipelineError};

/// Which elaboration-cache tier is present for a module (highest tier wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEntryKind {
    /// Full-output entry present (ADR 12.5.26a) — carries `CoreDef` bodies.
    FullOutput,
    /// Signature-only entry present (ADR 10.5.26n) — no bodies (the 4.7.26c hazard).
    SignatureOnly,
    /// No elaboration-cache entry for this module (fresh elaboration on next build).
    Uncached,
}

impl CacheEntryKind {
    /// Human-readable label used in the table + JSON output.
    pub fn label(self) -> &'static str {
        match self {
            CacheEntryKind::FullOutput => "full-output",
            CacheEntryKind::SignatureOnly => "signature-only",
            CacheEntryKind::Uncached => "(uncached)",
        }
    }

    /// Would this entry serve `CoreDef` bodies to a `run`/`test`?
    ///
    /// Signature-only is the sole "No" — it stores no bodies. Full-output does,
    /// and an uncached module is elaborated fresh (bodies produced directly), so
    /// both are "yes". This is an intrinsic property of the entry, independent of
    /// the current consumption policy (which, post-4.7.26c, skips the signature
    /// tier for `run`/`test` entirely).
    pub fn serves_bodies(self) -> bool {
        !matches!(self, CacheEntryKind::SignatureOnly)
    }
}

/// One row of `tungsten cache inspect` output: a module and its cache tier.
#[derive(Debug, Clone)]
pub struct ModuleCacheRow {
    /// Display name (source path relative to the project root, `.tg` stripped).
    pub module: String,
    /// Highest cache tier present for the module.
    pub kind: CacheEntryKind,
    /// Cached def count, if an entry is present (`None` when uncached).
    pub def_count: Option<usize>,
    /// First 8 hex chars of the signature cache key (for cross-referencing).
    pub hash_prefix: String,
}

/// Inspect the elaboration-cache tier of every module in a project (ADR 4.7.26d).
///
/// Runs Stub Registration + Signature Collection (to reproduce the global exports hash that keys each
/// module's Body Elaboration cache entry) but skips the expensive Body Elaboration body
/// elaboration, then probes the on-disk cache for each module. Read-only: no
/// cache writes, no new cache format. Cost 3 (parse + signature collection).
pub fn inspect_cache(path: &Path, verbose: bool) -> Result<Vec<ModuleCacheRow>, PipelineError> {
    // Canonicalize so a bare filename (whose `Path::parent` is `Some("")`, not a
    // readable directory) yields a real project root, and so the per-module
    // content hash uses the same absolute path the real build's cache reads do.
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path = path.as_path();
    let project_root = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let cache = BuildCache::new(&project_root, verbose)
        .map_err(|e| PipelineError::ElabFailed(format!("could not open cache: {e}")))?;
    let cache_mutex = Mutex::new(cache);

    // Parse the project; Signature Collection needs the same module_info/source_map the real
    // build uses so the exports hash matches byte-for-byte.
    let prepared = prepare_project(path, verbose, Some(&cache_mutex))?;
    let build = pipeline::BuildCtx {
        cache: Some(&cache_mutex),
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };

    Ok(probe_cache_tiers(
        &prepared.module_tree,
        &build,
        &cache_mutex,
        &project_root,
        verbose,
    ))
}

/// Probe every module's cache tier without running Body Elaboration (ADR 4.7.26d §2.1).
///
/// `cache` is locked only for the disk reads, after Signature Collection completes, so the
/// `BuildCtx`'s reference to the same cache cannot deadlock.
pub(in crate::driver) fn probe_cache_tiers(
    module_tree: &ParsedModule,
    build: &BuildCtx<'_>,
    cache: &Mutex<BuildCache>,
    project_root: &Path,
    verbose: bool,
) -> Vec<ModuleCacheRow> {
    // Stub Registration + Signature Collection reproduce the single global exports hash that keys every
    // module's Body Elaboration cache entry (per_module/mod.rs). Body Elaboration is skipped.
    let mut acc = ModuleTreeAccumulator::new();
    stubs::collect_all_type_and_constructor_stubs(module_tree, &mut acc.exports);
    phases::run_signature_collection(module_tree, build, &mut acc, verbose);
    let exports_hash = elab_cache::hash_exports(&acc.exports);

    let mut modules: Vec<&ParsedModule> = Vec::new();
    collect_modules(module_tree, &mut modules);

    let cache = cache.lock().unwrap();
    modules
        .iter()
        .map(|m| probe_one(m, &exports_hash, &cache, project_root))
        .collect()
}

/// Collect every module in the tree (post-order — children before parents),
/// matching the Body Elaboration elaboration order that writes cache entries.
fn collect_modules<'a>(module: &'a ParsedModule, out: &mut Vec<&'a ParsedModule>) {
    for child in &module.submodules {
        collect_modules(child, out);
    }
    out.push(module);
}

/// Probe a single module: recompute its keys and read whichever tier exists.
fn probe_one(
    module: &ParsedModule,
    exports_hash: &[u8; 32],
    cache: &BuildCache,
    project_root: &Path,
) -> ModuleCacheRow {
    let content = std::fs::read(&module.path).unwrap_or_default();
    let content_hash = elab_cache::hash_file_content(&content);
    let sig_key =
        elab_cache::compute_module_cache_key(COMPILER_VERSION, &content_hash, exports_hash);
    let full_key = elab_cache::compute_full_output_cache_key(&sig_key, FULL_OUTPUT_SCHEMA_VERSION);

    let module_name = display_name(&module.path, project_root);
    let hash_prefix = hex_string(&sig_key)[..8].to_string();

    // Full-output is the higher tier; check it first.
    if let Some(full) = cache.get_module_full_output(&full_key) {
        return ModuleCacheRow {
            module: module_name,
            kind: CacheEntryKind::FullOutput,
            def_count: Some(full.defs.len()),
            hash_prefix,
        };
    }
    if let Some(sig) = cache.get_module_elab(&sig_key) {
        return ModuleCacheRow {
            module: module_name,
            kind: CacheEntryKind::SignatureOnly,
            def_count: Some(sig.def_count),
            hash_prefix,
        };
    }
    ModuleCacheRow {
        module: module_name,
        kind: CacheEntryKind::Uncached,
        def_count: None,
        hash_prefix,
    }
}

/// Render a module's display name: path relative to the project root with the
/// `.tg` extension stripped and separators normalized to `/`.
fn display_name(path: &Path, project_root: &Path) -> String {
    let rel = path.strip_prefix(project_root).unwrap_or(path);
    rel.with_extension("").to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_bodies_only_signature_is_no() {
        assert!(CacheEntryKind::FullOutput.serves_bodies());
        assert!(CacheEntryKind::Uncached.serves_bodies());
        assert!(!CacheEntryKind::SignatureOnly.serves_bodies());
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(CacheEntryKind::FullOutput.label(), "full-output");
        assert_eq!(CacheEntryKind::SignatureOnly.label(), "signature-only");
        assert_eq!(CacheEntryKind::Uncached.label(), "(uncached)");
    }

    #[test]
    fn display_name_strips_root_and_extension() {
        let root = Path::new("/proj/src");
        let path = Path::new("/proj/src/compiler/main.tg");
        assert_eq!(display_name(path, root), "compiler/main");
    }

    #[test]
    fn display_name_falls_back_to_full_path_off_root() {
        let root = Path::new("/other");
        let path = Path::new("/proj/m.tg");
        // strip_prefix fails → uses the path itself, extension stripped.
        assert_eq!(display_name(path, root), "/proj/m");
    }
}
