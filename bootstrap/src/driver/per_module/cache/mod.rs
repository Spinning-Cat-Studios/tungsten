//! Elaboration cache integration for per-module elaboration (ADR 10.5.26l, 10.5.26n, 12.5.26a).
//!
//! Extracted from `mod.rs` to keep the per_module directory under complexity limits.

pub(in crate::driver) mod equivalence;
pub(crate) mod levels;

use crate::cache::elab_cache::{self, CachedModuleFullOutput, CachedModuleSignature};
use crate::cache::FULL_OUTPUT_SCHEMA_VERSION;
use crate::elaborate::{ElabMode, ElabOutput, ModuleExports};

use super::ElabTreeCtx;
use crate::driver::modules::ParsedModule;
use crate::driver::pipeline::BuildCtx;

/// Try to load a module's signature from cache.
///
/// Returns `Some(cached)` on hit, `None` on miss.
///
/// The signature-only cache (ADR 10.5.26n) stores no `CoreDef` bodies —
/// `CachedModuleSignature::into_elab_output()` yields `defs: Vec::new()`. Modes
/// that must evaluate bodies (`run` → `Compile`, `test` → `Test`) would inherit
/// an empty def list on a hit, so `run` can't find `main` (spurious `E0030` at
/// the EOF span) and `test` reports "no tests found" (ADR 4.7.26c). Only
/// `Check`, which never inspects bodies, may consume it. The full-output tier
/// (ADR 12.5.26a) is checked separately, *before* this, and does carry bodies.
///
/// **The mode list above is a consequence, not the rule.** The rule is general
/// and easy to re-break: *on a hit, this module's definitions are absent from
/// the accumulated set for the rest of the run*. Anything that later reasons
/// over the whole project's `defs` — a gate, an audit, a census — silently sees
/// a smaller environment, and "smaller" almost always reads as "fine". The
/// termination gate (ADR 29.6.26e) was written against the assembled set and
/// certified it: the same file was rejected on run 1 and clean on run 2, with
/// the definition *count* still printed correctly because that comes from
/// `cached_def_count`, not from `defs`. Its fix — cache the module's
/// conclusions (`CachedTermination`) rather than skip the check — is the shape
/// any future whole-project consumer needs.
/// See `docs/repo-memory/claude-memories/a-warm-cache-can-erase-a-gates-input.md`.
pub(super) fn try_cache_hit(
    module: &ParsedModule,
    exports_hash: &[u8; 32],
    build: &BuildCtx<'_>,
    elab_mode: ElabMode,
) -> Option<CachedModuleSignature> {
    if elab_mode != ElabMode::Check {
        return None;
    }
    let cache = build.cache.as_ref()?;
    let cache = cache.lock().unwrap();

    // Read source file content for hashing
    let content = std::fs::read(&module.path).ok()?;
    let content_hash = elab_cache::hash_file_content(&content);

    let compiler_version = crate::cache::COMPILER_VERSION;
    let cache_key =
        elab_cache::compute_module_cache_key(compiler_version, &content_hash, exports_hash);

    cache.get_module_elab(&cache_key)
}

/// Try to load a module's full-output cache entry (ADR 12.5.26a).
///
/// Returns `Some(cached)` on hit, `None` on miss or failure.
/// On failure, logs at debug level and returns `None` (graceful fallback).
pub(super) fn try_full_output_hit(
    module: &ParsedModule,
    ctx: &ElabTreeCtx<'_>,
) -> Option<CachedModuleFullOutput> {
    if !ctx.flags.full_output_cache {
        return None;
    }
    let cache_ref = ctx.build.cache.as_ref()?;
    let cache = cache_ref.lock().unwrap();

    let content = std::fs::read(&module.path).ok()?;
    let content_hash = elab_cache::hash_file_content(&content);
    let compiler_version = crate::cache::COMPILER_VERSION;
    let sig_key =
        elab_cache::compute_module_cache_key(compiler_version, &content_hash, &ctx.exports_hash);
    let full_key = elab_cache::compute_full_output_cache_key(&sig_key, FULL_OUTPUT_SCHEMA_VERSION);

    cache.get_module_full_output(&full_key)
}

/// A module's cache entry, staged during the walk and committed only if the
/// whole run succeeded (ADR 14.8.26g D5).
///
/// Buffering rather than writing in place is what keeps a failing run from
/// caching a module elaborated against a partial environment: the writer's
/// poison refusal cannot see a poison-free *value* whose type was merely
/// inferred through a poison comparison, and `hash_exports` fingerprints the
/// values half by name alone, so such an entry would be durable and silently
/// re-usable on a later clean run.
pub(super) struct PendingCacheWrite {
    cache_key: [u8; 32],
    signature: CachedModuleSignature,
    /// Pre-serialized full-output entry (ADR 12.5.26a), when enabled.
    full_output: Option<(std::path::PathBuf, Vec<u8>)>,
}

/// Stage a module's delta signature for the end-of-run commit
/// (ADR 10.5.26n cache tier; deferred-commit semantics ADR 14.8.26g D5).
///
/// Only stores delta exports (entries added by this module, not inherited from
/// prior modules). This keeps each cache entry small (~200B–2KB) instead of
/// storing the full accumulated environment (~1.8MB). Returns `None` when no
/// cache is configured.
pub(super) fn stage_module_cache_write(
    module: &ParsedModule,
    ctx: &ElabTreeCtx<'_>,
    output: &ElabOutput,
    exports: (&ModuleExports, &ModuleExports), // (new_exports, prior_exports)
) -> Option<PendingCacheWrite> {
    let (new_exports, prior_exports) = exports;
    let cache_ref = ctx.build.cache.as_ref()?;
    let content = std::fs::read(&module.path).unwrap_or_default();
    let content_hash = elab_cache::hash_file_content(&content);
    let compiler_version = crate::cache::COMPILER_VERSION;
    let cache_key =
        elab_cache::compute_module_cache_key(compiler_version, &content_hash, &ctx.exports_hash);
    let signature = CachedModuleSignature::from_output(output, new_exports, prior_exports);

    // Serialize (+ optionally compress) the full-output entry now, on the
    // elaborating thread (ADR 12.5.26a, 10.5.26o); only the disk write waits
    // for the commit.
    let full_output = if ctx.flags.full_output_cache {
        let full_key =
            elab_cache::compute_full_output_cache_key(&cache_key, FULL_OUTPUT_SCHEMA_VERSION);
        let full_entry = CachedModuleFullOutput::from_output(output, new_exports, prior_exports);
        let cache = cache_ref.lock().unwrap();
        match cache.serialize_full_output_entry(&full_key, &full_entry) {
            Ok((path, bytes)) => Some((path, bytes)),
            Err(e) => {
                if ctx.flags.verbose {
                    eprintln!("  [elab-cache-full] serialize failed: {e}");
                }
                None
            }
        }
    } else {
        None
    };

    Some(PendingCacheWrite {
        cache_key,
        signature,
        full_output,
    })
}

/// Commit the staged cache writes after a clean run (ADR 14.8.26g D5).
///
/// Called only when the walk accumulated no errors — a run that had a failure
/// commits nothing, including its cleanly-elaborated siblings, because "skip
/// after the first failure" is not well-defined under the parallel walker.
/// Writes are sorted by cache key so the commit order does not depend on the
/// order the walk staged them in.
pub(super) fn commit_pending_cache_writes(
    mut writes: Vec<PendingCacheWrite>,
    ctx: &ElabTreeCtx<'_>,
) {
    let Some(cache_ref) = ctx.build.cache.as_ref() else {
        return;
    };
    writes.sort_by(|a, b| a.cache_key.cmp(&b.cache_key));
    for write in writes {
        if let Err(e) = cache_ref
            .lock()
            .unwrap()
            .put_module_elab(&write.cache_key, &write.signature)
        {
            if ctx.flags.verbose {
                eprintln!("  [elab-cache] write failed: {e}");
            }
        }
        if let Some((path, bytes)) = write.full_output {
            if let Some(writer) = ctx.bg_writer {
                // Background write (ADR 10.5.26o)
                writer.send(path, bytes);
            } else {
                // Synchronous fallback (no background writer available)
                let elab_dir = path.parent().unwrap();
                let _ = std::fs::create_dir_all(elab_dir);
                if let Err(e) = std::fs::write(&path, &bytes) {
                    if ctx.flags.verbose {
                        eprintln!("  [elab-cache-full] write failed: {e}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! The mode guard on `try_cache_hit` (ADR 4.7.26c). The signature-only
    //! cache carries no bodies, so only `Check` may consume it; `Run`/`Test`
    //! (which lower to `Compile`/`Test`) must skip it and re-elaborate.

    use super::{elab_cache, try_cache_hit, BuildCtx, ParsedModule};
    use crate::ast::{SourceFile, Visibility};
    use crate::cache::elab_cache::CachedModuleSignature;
    use crate::cache::BuildCache;
    use crate::cache::COMPILER_VERSION;
    use crate::driver::modules::{ModuleInfo, SourceMap};
    use crate::elaborate::{ElabMode, ModuleExports};
    use crate::span::Span;
    use std::sync::Mutex;

    /// Populate a signature-cache entry for a real on-disk module and return the
    /// pieces `try_cache_hit` needs: the cache, module, and exports hash.
    fn seed_signature_entry(dir: &std::path::Path) -> (Mutex<BuildCache>, ParsedModule, [u8; 32]) {
        let path = dir.join("m.tg");
        let content = b"fn main() -> Nat { 0 }\n";
        std::fs::write(&path, content).unwrap();

        let cache = BuildCache::new(dir, false).unwrap();
        let exports_hash = [7u8; 32];
        let content_hash = elab_cache::hash_file_content(content);
        let key =
            elab_cache::compute_module_cache_key(COMPILER_VERSION, &content_hash, &exports_hash);
        let signature = CachedModuleSignature {
            delta_exports: ModuleExports::default(),
            warnings: Vec::new(),
            def_count: 1,
            termination: Default::default(),
        };
        cache.put_module_elab(&key, &signature).unwrap();

        let module = ParsedModule {
            path,
            visibility: Visibility::Private,
            source_file: SourceFile {
                items: Vec::new(),
                span: Span::default(),
            },
            submodules: Vec::new(),
        };
        (Mutex::new(cache), module, exports_hash)
    }

    fn build_ctx(cache: &Mutex<BuildCache>) -> BuildCtx<'_> {
        BuildCtx {
            cache: Some(cache),
            module_info: ModuleInfo::default(),
            source_map: SourceMap::single(std::path::PathBuf::from("m.tg"), String::new()),
        }
    }

    #[test]
    fn check_mode_consumes_signature_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (cache, module, exports_hash) = seed_signature_entry(dir.path());
        let build = build_ctx(&cache);
        // Check needs no bodies → the signature hit is sound and taken.
        assert!(
            try_cache_hit(&module, &exports_hash, &build, ElabMode::Check).is_some(),
            "check mode must still hit the signature cache"
        );
    }

    #[test]
    fn body_needing_modes_skip_signature_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (cache, module, exports_hash) = seed_signature_entry(dir.path());
        let build = build_ctx(&cache);
        // Compile (run) and Test would inherit an empty def list from a bodyless
        // hit → they must skip the read even though a matching entry exists.
        for mode in [ElabMode::Compile, ElabMode::Test] {
            assert!(
                try_cache_hit(&module, &exports_hash, &build, mode).is_none(),
                "{mode:?} must not consume the bodyless signature cache",
            );
        }
    }
}
