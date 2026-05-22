//! Per-module body elaboration (Phase B per-module step).
//!
//! Extracted from `per_module/mod.rs` to reduce structural complexity.
//! Contains the single-module elaboration logic: cache check, fresh
//! elaboration, and result accumulation.

use std::time::{Duration, Instant};

use crate::ast::{Item, SourceFile};
use crate::elaborate::{ElabError, ElabOutput, ModuleExports};
use tungsten_core::Context;

use crate::cache::CachedModuleFullOutput;

use super::accumulator::ModuleTreeAccumulator;
use super::modules::ParsedModule;
use super::ElabTreeCtx;
use super::TreeWalkState;
use super::{cache, profile};

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
    if !output.defs.is_empty() {
        state.acc.module_defs.push((
            module_path.to_vec(),
            module.path.clone(),
            output.defs.clone(),
        ));
    }
    if ctx.trace.trace_ctor_registration {
        for (name, info) in &exports.constructors {
            eprintln!(
                "[ctor-reg] Phase B: register {} (parent={}, index={}) via full-output cache",
                name, info.type_name, info.index
            );
        }
    }
    state.acc.merge_output(output);
    state.acc.merge_exports(exports);
}

/// Build a mini AST for a single module (excluding `mod` declarations).
pub(super) fn build_module_mini_ast(module: &ParsedModule) -> SourceFile {
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
pub(super) fn elaborate_self(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(), Vec<ElabError>> {
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

    // Fall back to signature-only cache (ADR 10.5.26n) — no bodies
    let (output, new_exports) = match cache::try_cache_hit(module, &ctx.exports_hash, ctx.build) {
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
        None => {
            if ctx.flags.verbose {
                eprintln!(
                    "  Elaborating module {:?} ({} items)",
                    module.path.display(),
                    mini_ast.items.len()
                );
            }
            let (output, exports, timing) =
                elaborate_module_fresh(module, &mini_ast, module_path, ctx, &state.acc.exports)?;
            if ctx.flags.profiling {
                state.elab_profile.record_module(timing);
            }
            (output, exports)
        }
    };

    // 5. Record per-module defs for codegen unit partitioning (ADR 7.5.26h)
    if !output.defs.is_empty() {
        state.acc.module_defs.push((
            module_path.to_vec(),
            module.path.clone(),
            output.defs.clone(),
        ));
    }

    // 6. Accumulate results and exports
    if ctx.trace.trace_ctor_registration {
        for (name, info) in &new_exports.constructors {
            eprintln!(
                "[ctor-reg] Phase B: register {} (parent={}, index={}) via elaborate_with_exports",
                name, info.type_name, info.index
            );
        }
    }
    state.acc.merge_output(output);
    state.acc.merge_exports(new_exports);

    Ok(())
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

    // Write to cache on success (ADR 10.5.26l §P5).
    let t_cache_start = Instant::now();
    cache::write_module_to_cache(module, ctx, &output, (&new_exports, prior_exports));
    let t_cache = t_cache_start.elapsed();

    if ctx.flags.verbose {
        eprintln!(
            "    sub-phases: collect={:.1?}, body={:.1?}, cache_write={:.1?}",
            t_collect, t_body, t_cache,
        );
    }

    let timing = profile::ModuleTiming {
        path: module.path.display().to_string(),
        collection: t_collect,
        body: t_body,
        cache_write: t_cache,
        cache_hit: false,
    };

    Ok((output, new_exports, timing))
}
