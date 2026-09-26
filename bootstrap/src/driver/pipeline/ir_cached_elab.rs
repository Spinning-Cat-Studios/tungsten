//! Single-file elaboration with the IR cache (the non-module-tree path).
//!
//! Split from `pipeline.rs` by the ADR 14.8.26g retrospective, which took that
//! file over its size threshold. Multi-file projects go through
//! `per_module::elaborate_module_tree` instead (ADR 5.5.26c); this is the
//! hybrid path that runs the collection pass, keys the IR cache on the
//! resulting types hash, and elaborates only on a miss.

use std::path::Path;

use tungsten_core::Context;

use super::BuildCtx;
use crate::ast::SourceFile;
use crate::cache::BuildCache;
use crate::driver::output::TraceOptions;
use crate::elaborate::{collect_definitions, ElabError, ElabOutput};

/// Elaborate with IR caching using the hybrid approach.
///
/// The hybrid approach:
/// 1. Always run the collection pass (~10% of elaboration time)
/// 2. Compute types_hash from collected type definitions
/// 3. If cache hit: return cached CoreDefs (no warnings since we didn't elaborate)
/// 4. If cache miss: continue with elaboration and cache the result
///
/// Note: This is used for single-file runs (no module tree). Multi-file projects
/// use `per_module::elaborate_module_tree` instead (ADR 5.5.26c).
pub(in crate::driver) fn elaborate_with_ir_cache(
    ast: &SourceFile,
    source_path: &Path,
    verbose: bool,
    build: &BuildCtx<'_>,
    trace: &TraceOptions,
) -> Result<ElabOutput, Vec<ElabError>> {
    use crate::elaborate::collect_definitions_with_modules;

    let mut ctx = Context::new();

    // If no cache, just elaborate directly
    let cache = match build.cache {
        Some(c) => c,
        None => {
            let output = if build.module_info.modules.is_empty() {
                let mut collected = collect_definitions(ast, &mut ctx)?;
                collected.apply_trace_options(trace);
                collected.elaborate()?
            } else {
                // With module info - use the module-aware collection
                let mut collected =
                    collect_definitions_with_modules(ast, &mut ctx, build.module_info.clone())?;
                collected.apply_trace_options(trace);
                collected.elaborate()?
            };
            return Ok(output);
        }
    };

    // Step 1: Run collection pass (always runs - ~10% of time)
    let collected = if build.module_info.modules.is_empty() {
        collect_definitions(ast, &mut ctx)?
    } else {
        collect_definitions_with_modules(ast, &mut ctx, build.module_info.clone())?
    };

    // Step 2: Compute types_hash from collected types
    let types = collected.types_for_hash();
    let types_hash = BuildCache::compute_types_hash(&types);

    // Step 3: Check IR cache — only when collection was clean. The pass
    // defers its errors instead of returning them (ADR 14.8.26g D2), and a
    // collection error does not necessarily move the types hash, so a stale
    // hit here would mask the failure entirely. Fall through to `elaborate()`,
    // which drains the deferred errors and fails the run.
    if !collected.has_collection_errors() {
        if let Some(cached_defs) = cache.lock().unwrap().get_ir(source_path, &types_hash) {
            if verbose {
                eprintln!(
                    "Using cached elaboration ({} definitions)",
                    cached_defs.len()
                );
            }
            // Cache hit - return cached defs (no warnings since we didn't elaborate)
            // Note: record_types and adt_types are not cached, so we return empty for cached results
            // This is fine for non-compile use cases (check, run)
            return Ok(ElabOutput {
                defs: cached_defs,
                ..ElabOutput::default()
            });
        }
    }

    // Step 4: Cache miss - continue with elaboration
    let mut collected = collected;
    collected.apply_trace_options(trace);
    let output = collected.elaborate()?;

    // Step 5: Cache the result
    if let Err(e) = cache
        .lock()
        .unwrap()
        .put_ir(source_path, types_hash, &output.defs)
    {
        if let Some(notice) = ir_cache_write_notice(&e, verbose) {
            eprintln!("{notice}");
        }
    }

    Ok(output)
}

/// What to say about an IR-cache write that failed, or `None` for silence.
///
/// `put_ir` refuses a file whose AST was never cached ("put AST first"), and
/// that `NotFound` is a **sequencing bug in the caller**, not an environmental
/// one: the write could never have succeeded, so every later run re-elaborates
/// while believing it populated a cache. It is reported whatever the
/// verbosity. Genuine I/O failures (disk, permissions) are environmental and
/// stay behind `--verbose`.
///
/// The distinction is not academic — it cost ADR 14.8.26g a test: a fixture
/// that "seeded the cache" without seeding the AST entry silently exercised a
/// permanent miss, and only a surviving mutant on the clean-collection guard
/// exposed it. A pure function rather than an inline `if` so both arms are
/// assertable; an `eprintln!` behind `verbose` is invisible to the suite.
fn ir_cache_write_notice(error: &std::io::Error, verbose: bool) -> Option<String> {
    if error.kind() == std::io::ErrorKind::NotFound {
        return Some(format!(
            "[cache] warning: IR not cached — {error}. This file's AST was never \
             cached, so nothing will hit on a later run."
        ));
    }
    verbose.then(|| format!("[cache] warning: failed to cache IR: {error}"))
}

#[cfg(test)]
mod tests {
    use super::ir_cache_write_notice;
    use std::io::{Error, ErrorKind};

    /// The precondition failure is a caller sequencing bug — reported at any
    /// verbosity, because a silent one means every later run re-elaborates
    /// while believing it populated a cache.
    #[test]
    fn a_missing_ast_entry_is_reported_even_when_quiet() {
        let err = Error::new(ErrorKind::NotFound, "no AST cache entry for this file");
        let notice = ir_cache_write_notice(&err, false).expect("must speak up when quiet");
        assert!(notice.contains("AST was never"), "{notice}");
        assert!(
            ir_cache_write_notice(&err, true).is_some(),
            "and when verbose"
        );
    }

    /// Environmental failures stay behind `--verbose`: nothing the caller did
    /// wrong, and a disk error on a *cache* write must not become noise on
    /// every ordinary run.
    #[test]
    fn an_io_failure_stays_behind_verbose() {
        let err = Error::new(ErrorKind::PermissionDenied, "read-only filesystem");
        assert!(
            ir_cache_write_notice(&err, false).is_none(),
            "quiet by default"
        );
        let notice = ir_cache_write_notice(&err, true).expect("verbose must report it");
        assert!(notice.contains("read-only filesystem"), "{notice}");
    }
}
