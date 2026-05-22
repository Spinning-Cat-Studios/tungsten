//! Elaboration cache integration for per-module elaboration (ADR 10.5.26l, 10.5.26n, 12.5.26a).
//!
//! Extracted from `mod.rs` to keep the per_module directory under complexity limits.

pub(in crate::driver) mod equivalence;
pub(crate) mod levels;

use crate::cache::elab_cache::{self, CachedModuleFullOutput, CachedModuleSignature};
use crate::cache::FULL_OUTPUT_SCHEMA_VERSION;
use crate::elaborate::{ElabOutput, ModuleExports};

use super::ElabTreeCtx;
use crate::driver::modules::ParsedModule;
use crate::driver::pipeline::BuildCtx;

/// Try to load a module's signature from cache.
///
/// Returns `Some(cached)` on hit, `None` on miss.
pub(super) fn try_cache_hit(
    module: &ParsedModule,
    exports_hash: &[u8; 32],
    build: &BuildCtx<'_>,
) -> Option<CachedModuleSignature> {
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

/// Write a module's delta signature to cache (ADR 10.5.26n — always enabled).
///
/// Only stores delta exports (entries added by this module, not inherited from
/// prior modules). This keeps each cache entry small (~200B–2KB) instead of
/// storing the full accumulated environment (~1.8MB).
pub(super) fn write_module_to_cache(
    module: &ParsedModule,
    ctx: &ElabTreeCtx<'_>,
    output: &ElabOutput,
    exports: (&ModuleExports, &ModuleExports), // (new_exports, prior_exports)
) {
    let (new_exports, prior_exports) = exports;
    if let Some(cache_ref) = ctx.build.cache.as_ref() {
        let content = std::fs::read(&module.path).unwrap_or_default();
        let content_hash = elab_cache::hash_file_content(&content);
        let compiler_version = crate::cache::COMPILER_VERSION;
        let cache_key = elab_cache::compute_module_cache_key(
            compiler_version,
            &content_hash,
            &ctx.exports_hash,
        );
        let signature = CachedModuleSignature::from_output(output, new_exports, prior_exports);
        if let Err(e) = cache_ref
            .lock()
            .unwrap()
            .put_module_elab(&cache_key, &signature)
        {
            if ctx.flags.verbose {
                eprintln!("  [elab-cache] write failed: {e}");
            }
        }

        // Also write full-output entry if enabled (ADR 12.5.26a, 10.5.26o)
        if ctx.flags.full_output_cache {
            let full_key =
                elab_cache::compute_full_output_cache_key(&cache_key, FULL_OUTPUT_SCHEMA_VERSION);
            let full_entry =
                CachedModuleFullOutput::from_output(output, new_exports, prior_exports);

            // Serialize (+ optionally compress) on this thread, then send to background writer
            let cache = cache_ref.lock().unwrap();
            match cache.serialize_full_output_entry(&full_key, &full_entry) {
                Ok((path, bytes)) => {
                    drop(cache); // Release lock before sending
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
                Err(e) => {
                    if ctx.flags.verbose {
                        eprintln!("  [elab-cache-full] serialize failed: {e}");
                    }
                }
            }
        }
    }
}
