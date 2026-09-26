//! Per-module body elaboration (Body Elaboration per-module step).
//!
//! Extracted from `per_module/mod.rs` to reduce structural complexity.
//! Contains the single-module elaboration logic: cache check, fresh
//! elaboration, and result accumulation.

use std::time::{Duration, Instant};

use crate::ast::{Item, SourceFile};
use crate::elaborate::{ElabError, ElabOutput, ModuleExports};
use tungsten_core::Context;

use crate::cache::CachedModuleFullOutput;

use super::TreeWalkState;
use crate::driver::per_module::accumulator::ModuleTreeAccumulator;
use crate::driver::per_module::modules::ParsedModule;
use crate::driver::per_module::ElabTreeCtx;
use crate::driver::per_module::{cache, profile};

/// Apply a full-output cache hit: merge defs, exports, and record profiling.
fn apply_full_output_hit(
    full_cached: CachedModuleFullOutput,
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) {
    if ctx.flags.verbose {
        eprintln!(
            "  [elab-cache-full] hit for {:?} ({} defs)",
            module.path.display(),
            full_cached.defs.len(),
        );
    }
    state.acc.cached_def_count += full_cached.defs.len();
    let exports = full_cached.delta_exports.clone();
    if ctx.flags.profiling {
        state.elab_profile.record_module(profile::ModuleTiming {
            path: module.path.display().to_string(),
            collection: Duration::ZERO,
            body: Duration::ZERO,
            cache_write: Duration::ZERO,
            cache_hit: true,
        });
    }
    let output = full_cached.into_elab_output();
    let canonical_path = canonical_module_path(module_path, ctx);
    record_module_import_targets(&canonical_path, &output, state);
    if !output.defs.is_empty() {
        state
            .acc
            .module_defs
            .push((canonical_path, module.path.clone(), output.defs.clone()));
    }
    if ctx.trace.trace_ctor_registration {
        for (name, info) in &exports.constructors {
            eprintln!(
                "[ctor-reg] Body Elaboration: register {} (parent={}, index={}) via full-output cache",
                name, info.type_name, info.index
            );
        }
    }
    state.acc.merge_output(output);
    state.acc.merge_exports(exports);
}

/// Canonicalize a tree-walk module path through the driver's file mapping
/// (ADR 12.7.26a §2.1): when workspace siblings register one module under two
/// prefixes (`main::parser` vs `parser`), the first-registered path per source
/// file wins, so import-target keys and `DefInfo.module_path` compare equal.
fn canonical_module_path(module_path: &[String], ctx: &ElabTreeCtx<'_>) -> Vec<String> {
    let raw = crate::elaborate::ModulePath::new(module_path.to_vec());
    ctx.build.module_info.canonicalize_path(&raw).segments
}

/// Record this module's value import targets for codegen (ADR 12.7.26a §2.1).
/// Empty tables are skipped — a module with no value imports needs no entry.
fn record_module_import_targets(
    canonical_path: &[String],
    output: &crate::elaborate::ElabOutput,
    state: &mut TreeWalkState<'_>,
) {
    if output.value_import_targets.is_empty() {
        return;
    }
    state
        .acc
        .module_import_targets
        .insert(canonical_path.to_vec(), output.value_import_targets.clone());
}

/// Build a mini AST for a single module (excluding `mod` declarations).
pub(in crate::driver::per_module) fn build_module_mini_ast(module: &ParsedModule) -> SourceFile {
    let items: Vec<Item> = module
        .source_file
        .items
        .iter()
        .filter(|item| !matches!(item, Item::Mod(_)))
        .cloned()
        .collect();
    SourceFile {
        items,
        span: module.source_file.span,
    }
}

/// Elaborate this module itself (steps 3–6 from the original monolithic function).
pub(in crate::driver::per_module) fn elaborate_self(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(), Vec<ElabError>> {
    // Count before any early return: this measures where the walk got to, and
    // an empty module or a cache hit is still somewhere it got to.
    state.acc.note_module_walked();

    let mini_ast = build_module_mini_ast(module);

    if mini_ast.items.is_empty() {
        return Ok(());
    }

    // 4. Check elaboration cache (ADR 10.5.26l, 10.5.26n, 12.5.26a)
    let t_cache_read = Instant::now();

    // Try full-output cache first (ADR 12.5.26a) — includes CoreDef bodies
    if let Some(full_cached) = cache::try_full_output_hit(module, ctx) {
        apply_full_output_hit(full_cached, module, module_path, ctx, state);
        return Ok(());
    }

    // Fall back to signature-only cache (ADR 10.5.26n) — no bodies. Gated to
    // Check mode: run/test need CoreDef bodies and would get an empty def list
    // from a bodyless hit (ADR 4.7.26c).
    let (output, new_exports) =
        match cache::try_cache_hit(module, &ctx.exports_hash, ctx.build, ctx.trace.elab_mode) {
            Some(cached) => {
                let t_read = t_cache_read.elapsed();
                if ctx.flags.verbose {
                    eprintln!(
                        "  [elab-cache] hit for {:?} (read={:.1?})",
                        module.path.display(),
                        t_read,
                    );
                }
                state.acc.cached_def_count += cached.def_count;
                let exports = cached.delta_exports.clone();
                if ctx.flags.profiling {
                    state.elab_profile.record_module(profile::ModuleTiming {
                        path: module.path.display().to_string(),
                        collection: Duration::ZERO,
                        body: Duration::ZERO,
                        cache_write: Duration::ZERO,
                        cache_hit: true,
                    });
                }
                (cached.into_elab_output(), exports)
            }
            None => elaborate_fresh_and_stage(module, &mini_ast, module_path, ctx, state)?,
        };

    // 5. Record per-module defs for codegen unit partitioning (ADR 7.5.26h)
    // and the module's value import targets (ADR 12.7.26a §2.1). Both are
    // keyed by the CANONICAL module path so codegen's provenance lookups
    // compare in one path space (§2.1 — workspace siblings can register one
    // module under two prefixes).
    let canonical_path = canonical_module_path(module_path, ctx);
    record_module_import_targets(&canonical_path, &output, state);
    if !output.defs.is_empty() {
        state
            .acc
            .module_defs
            .push((canonical_path, module.path.clone(), output.defs.clone()));
    }

    // 6. Accumulate results and exports
    if ctx.trace.trace_ctor_registration {
        for (name, info) in &new_exports.constructors {
            eprintln!(
                "[ctor-reg] Body Elaboration: register {} (parent={}, index={}) via elaborate_with_exports",
                name, info.type_name, info.index
            );
        }
    }
    state.acc.merge_output(output);
    state.acc.merge_exports(new_exports);

    Ok(())
}

/// The cache-miss arm of [`elaborate_self`]: elaborate fresh, then stage the
/// cache entry — it commits only if the whole run succeeds (ADR 14.8.26g D5).
/// `state.acc.exports` is still this module's prior environment here — the
/// caller's merge is what changes it.
fn elaborate_fresh_and_stage(
    module: &ParsedModule,
    mini_ast: &SourceFile,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(ElabOutput, ModuleExports), Vec<ElabError>> {
    if ctx.flags.verbose {
        eprintln!(
            "  Elaborating module {:?} ({} items)",
            module.path.display(),
            mini_ast.items.len()
        );
    }
    let (output, exports, mut timing) =
        elaborate_module_fresh(module, mini_ast, module_path, ctx, &state.acc.exports)?;
    let t_stage = Instant::now();
    if let Some(pending) =
        cache::stage_module_cache_write(module, ctx, &output, (&exports, &state.acc.exports))
    {
        state.acc.pending_cache_writes.push(pending);
    }
    timing.cache_write = t_stage.elapsed();
    if ctx.flags.profiling {
        state.elab_profile.record_module(timing);
    }
    Ok((output, exports))
}

/// Elaborate a module from scratch (cache miss path).
///
/// Also writes the result to cache for future hits.
/// Returns (ElabOutput, new_exports, timing).
fn elaborate_module_fresh(
    module: &ParsedModule,
    mini_ast: &SourceFile,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    prior_exports: &ModuleExports,
) -> Result<(ElabOutput, ModuleExports, profile::ModuleTiming), Vec<ElabError>> {
    // Build per-module module info with correct item_index_to_file
    let mut module_info = ctx.build.module_info.clone();
    module_info.item_index_to_file = vec![module.path.clone(); mini_ast.items.len()];

    // Sub-phase timing (ADR 10.5.26n §P0)
    let t_collect_start = Instant::now();

    // Elaborate with injected exports from prior modules
    let mut local_ctx = Context::new();
    let mut collected = crate::elaborate::collect_definitions_with_exports(
        mini_ast,
        &mut local_ctx,
        module_info,
        prior_exports,
    )?;

    let t_collect = t_collect_start.elapsed();

    collected.apply_trace_options(ctx.trace);

    let t_body_start = Instant::now();
    let (output, new_exports) = collected.elaborate_with_exports()?;
    let t_body = t_body_start.elapsed();

    if ctx.flags.verbose {
        eprintln!(
            "    sub-phases: collect={:.1?}, body={:.1?}",
            t_collect, t_body,
        );
    }

    // The cache entry is staged by the caller after this returns (ADR
    // 14.8.26g D5), which overwrites `cache_write` with the staging time.
    let timing = profile::ModuleTiming {
        path: module.path.display().to_string(),
        collection: t_collect,
        body: t_body,
        cache_write: Duration::ZERO,
        cache_hit: false,
    };

    Ok((output, new_exports, timing))
}
