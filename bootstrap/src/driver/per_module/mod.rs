//! Per-module elaboration loop (ADR 5.5.26b, 5.5.26c).
//!
//! Elaborates each module in the `ParsedModule` tree independently in
//! post-order (children before parents). Cross-module definitions from
//! completed modules are injected into subsequent modules' environments.
//!
//! Two-phase architecture (ADR 5.5.26c):
//!   Phase A — walk ALL modules, register type + constructor stubs globally
//!   Phase A.5 — collect ALL function signatures globally (combined AST)
//!   Phase B — elaborate each module's value bodies in post-order

mod accumulator;
mod body;
pub(super) mod cache;
mod profile;
pub(crate) mod stubs;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::time::Instant;

use crate::cache::elab_cache;
use crate::cache::elab_cache::writer::{self, BackgroundWriter};
use crate::elaborate::{ElabError, ElabOutput, ModuleExports};
use tungsten_core::Context;

use accumulator::ModuleTreeAccumulator;

use super::modules::{self, ParsedModule};
use super::output::TraceOptions;
use super::pipeline::BuildCtx;

/// Output of per-module elaboration, extending `ElabOutput` with module partitioning data.
pub(super) struct ModuleTreeOutput {
    pub(super) elab: ElabOutput,
    /// Per-module definition groups for codegen unit partitioning (ADR 7.5.26h).
    /// Each entry is (module_path, source_file, defs).
    pub(super) module_defs: Vec<(
        Vec<String>,
        std::path::PathBuf,
        Vec<crate::elaborate::CoreDef>,
    )>,
    /// Def count from cache hits (bodies not re-elaborated).
    pub(super) cached_def_count: usize,
}

/// Elaborate a module tree per-module in post-order (ADR 5.5.26b §3, 5.5.26c).
///
/// Two-phase approach:
///   Phase A: Walk all modules and register type + constructor stubs globally,
///            so cross-branch imports resolve before any body elaboration.
///   Phase B: Elaborate each module's value bodies in post-order, injecting
///            full defs from completed siblings.
///
/// Caching is not yet supported in per-module mode (ADR 5.5.26b non-goals).
pub(super) fn elaborate_module_tree(
    module_tree: &ParsedModule,
    _source_path: &Path,
    verbose: bool,
    build: &BuildCtx<'_>,
    trace: &TraceOptions,
) -> Result<ModuleTreeOutput, Vec<ElabError>> {
    let mut acc = ModuleTreeAccumulator::new();
    let profiling = profile::is_enabled();
    let mut elab_profile = profile::ElabProfile::new();

    // Phase A: collect type + constructor stubs from ALL modules (ADR 5.5.26c §2.2)
    let phase_a_start = Instant::now();
    stubs::collect_all_type_and_constructor_stubs(module_tree, &mut acc.exports);
    let phase_a_elapsed = phase_a_start.elapsed();
    elab_profile.phase_a = phase_a_elapsed;
    log_phase_a(verbose, trace, &acc.exports);

    // Phase A.5: collect function signatures globally.
    // Build a combined AST of ALL items from ALL modules and run the
    // collection pass using Phase A type stubs. This gives every module
    // access to all function types for cross-branch value imports.
    let phase_a5_start = Instant::now();
    run_phase_a5(module_tree, build, &mut acc, verbose);
    let phase_a5_elapsed = phase_a5_start.elapsed();
    log_phase_a5(verbose, &acc.exports);

    // Compute exports hash once for Phase B cache keys (ADR 10.5.26l §2.1).
    // This captures the full Phase A.5 environment state. Any upstream change
    // produces a different hash, conservatively invalidating all module caches.
    let exports_hash = elab_cache::hash_exports(&acc.exports);

    elab_profile.phase_a5 = phase_a5_elapsed;

    // Phase B: elaborate each module's bodies in post-order
    let full_output_cache = std::env::var("TUNGSTEN_ELAB_CACHE_FULL")
        .map(|v| v == "1")
        .unwrap_or(false);
    let phase_b_start = Instant::now();
    run_phase_b(
        module_tree,
        build,
        trace,
        ElabCtxFlags {
            verbose,
            profiling,
            full_output_cache,
        },
        exports_hash,
        &mut acc,
        &mut elab_profile,
    )?;
    let phase_b_elapsed = phase_b_start.elapsed();
    elab_profile.phase_b_total = phase_b_elapsed;

    if verbose {
        let total = phase_a_elapsed + phase_a5_elapsed + phase_b_elapsed;
        eprintln!(
            "  Elaboration phase timing: A={:.0?}, A.5={:.0?}, B={:.0?}, total={:.0?}",
            phase_a_elapsed, phase_a5_elapsed, phase_b_elapsed, total,
        );
    }

    if profiling {
        elab_profile.emit();
    }

    let cached_def_count = acc.cached_def_count;
    let module_defs = acc.module_defs.clone();
    Ok(ModuleTreeOutput {
        elab: acc.into_output(),
        module_defs,
        cached_def_count,
    })
}

/// Run Phase B: elaborate module bodies in post-order with optional background caching.
fn run_phase_b(
    module_tree: &ParsedModule,
    build: &BuildCtx<'_>,
    trace: &TraceOptions,
    flags: ElabCtxFlags,
    exports_hash: [u8; 32],
    acc: &mut ModuleTreeAccumulator,
    elab_profile: &mut profile::ElabProfile,
) -> Result<(), Vec<ElabError>> {
    // Spawn background cache writer for full-output entries (ADR 10.5.26o)
    let bg_writer = if flags.full_output_cache {
        Some(BackgroundWriter::spawn(writer::default_channel_capacity()))
    } else {
        None
    };

    let elab_ctx = build_elab_ctx(flags, build, trace, exports_hash, bg_writer.as_ref());
    let root_path: Vec<String> = Vec::new();
    {
        let mut state = TreeWalkState { acc, elab_profile };
        elaborate_module_tree_rec(module_tree, &root_path, &elab_ctx, &mut state)
    }
    .map_err(|mut errors| {
        // ADR 13.5.26g §2.2: annotate "not found" errors when Phase A.5 failed.
        if !acc.phase_a5_ok {
            annotate_errors_for_phase_a5_failure(&mut errors);
        }
        errors
    })?;

    // Join background writer — flush all pending entries (ADR 10.5.26o)
    if let Some(writer) = bg_writer {
        let write_errors = writer.join();
        if !write_errors.is_empty() && flags.verbose {
            eprintln!(
                "[elab-cache-full] {} background write error(s) during Phase B",
                write_errors.len()
            );
            for err in &write_errors {
                eprintln!("  {}: {}", err.path.display(), err.error);
            }
        }
    }

    Ok(())
}

/// Context for recursive module elaboration (bundles environment params).
pub(super) struct ElabTreeCtx<'a> {
    pub(super) flags: ElabCtxFlags,
    pub(super) build: &'a BuildCtx<'a>,
    trace: &'a TraceOptions,
    /// Hash of Phase A.5 exports for cache key computation (ADR 10.5.26l).
    pub(super) exports_hash: [u8; 32],
    /// Background cache writer for full-output entries (ADR 10.5.26o).
    /// `None` when full-output caching is disabled.
    pub(super) bg_writer: Option<&'a BackgroundWriter>,
    /// Configured thread count for parallel Phase B (ADR 11.5.26b §P5).
    /// Read once from `TUNGSTEN_ELAB_THREADS`; 1 = serial (default).
    thread_count: usize,
    /// Shared rayon thread pool for parallel Phase B (ADR 11.5.26b §P5).
    /// `None` when `thread_count == 1` (serial mode).
    parallel_pool: Option<rayon::ThreadPool>,
}

/// Boolean flags controlling per-module elaboration behavior.
#[derive(Clone, Copy)]
pub(super) struct ElabCtxFlags {
    pub(super) verbose: bool,
    /// Whether per-module profiling is enabled (ADR 11.5.26b §P0).
    pub(super) profiling: bool,
    /// Whether full-output caching is enabled (ADR 12.5.26a).
    pub(super) full_output_cache: bool,
}

fn build_elab_ctx<'a>(
    flags: ElabCtxFlags,
    build: &'a BuildCtx<'a>,
    trace: &'a TraceOptions,
    exports_hash: [u8; 32],
    bg_writer: Option<&'a BackgroundWriter>,
) -> ElabTreeCtx<'a> {
    let thread_count = cache::equivalence::elab_thread_count();
    let parallel_pool = if thread_count > 1 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .ok()
    } else {
        None
    };
    ElabTreeCtx {
        flags,
        build,
        trace,
        exports_hash,
        bg_writer,
        thread_count,
        parallel_pool,
    }
}

/// Log Phase A results (verbose + constructor trace).
fn log_phase_a(verbose: bool, trace: &TraceOptions, exports: &ModuleExports) {
    if verbose {
        eprintln!(
            "  Phase A: collected {} type stubs, {} constructor stubs",
            exports.types.len(),
            exports.constructors.len(),
        );
    }
    if trace.trace_ctor_registration {
        for (name, info) in &exports.constructors {
            eprintln!(
                "[ctor-reg] Phase A: register {} (parent={}, index={}) via collect_all_type_and_constructor_stubs",
                name, info.type_name, info.index
            );
        }
    }
}

/// Log Phase A.5 results (verbose).
fn log_phase_a5(verbose: bool, exports: &ModuleExports) {
    if verbose {
        eprintln!(
            "  Phase A.5: {} types, {} values, {} constructors after global collection",
            exports.types.len(),
            exports.values.len(),
            exports.constructors.len(),
        );
    }
}

/// Phase A.5: build combined AST from all modules and run global collection
/// to extract function signatures. Results are merged into `acc.exports`.
fn run_phase_a5(
    module_tree: &ParsedModule,
    build: &BuildCtx<'_>,
    acc: &mut ModuleTreeAccumulator,
    verbose: bool,
) {
    let (combined_ast, combined_file_index) = super::pipeline::build_combined_ast(module_tree);
    let mut combined_module_info = build.module_info.clone();
    combined_module_info.item_index_to_file = combined_file_index;

    let mut ctx = Context::new();
    match crate::elaborate::collect_definitions_with_exports(
        &combined_ast,
        &mut ctx,
        combined_module_info,
        &acc.exports,
    ) {
        Ok(collected) => {
            let global_exports = collected.extract_value_exports();
            if verbose {
                eprintln!(
                    "  Phase A.5: global collection succeeded, {} types, {} values, {} constructors",
                    global_exports.types.len(),
                    global_exports.values.len(),
                    global_exports.constructors.len(),
                );
            }
            acc.merge_exports(global_exports);
        }
        Err(errors) => {
            // Always warn on Phase A.5 failure (ADR 13.5.26g §2.1).
            // In verbose mode, also print individual errors.
            let count = errors.len();
            eprintln!(
                "warning: Phase A.5 global collection failed with {} error{}; \
                 cross-module imports may not resolve.",
                count,
                if count == 1 { "" } else { "s" },
            );
            if let Some(first) = errors.first() {
                eprintln!("  first error: {}", first);
            }
            eprintln!("  hint: run `tungsten doctor check phase-a5 <file>` for details");
            if verbose {
                for e in &errors {
                    eprintln!("    - {}", e);
                }
            }
            acc.phase_a5_ok = false;
        }
    }
}

/// Annotate "not found" errors with a Phase A.5 failure hint (ADR 13.5.26g §2.2).
///
/// When Phase A.5 global collection fails, downstream modules can't resolve
/// cross-module imports, producing misleading E0001/E0005/E0006 errors.
/// This adds a note to those errors pointing to the real root cause.
fn annotate_errors_for_phase_a5_failure(errors: &mut [ElabError]) {
    use crate::elaborate::ElabErrorKind;
    use crate::elaborate::Note;

    let hint = "Phase A.5 global collection failed — this error may be caused by \
                a bad import in another module. Run `tungsten doctor check phase-a5 <file>` \
                for details.";

    for err in errors.iter_mut() {
        let is_resolution_error = matches!(
            &err.kind,
            ElabErrorKind::UndefinedVariable(_)
                | ElabErrorKind::ModuleNotFound { .. }
                | ElabErrorKind::ItemNotFoundInModule { .. }
                | ElabErrorKind::UnresolvedImport(_)
        );
        if is_resolution_error {
            err.notes.push(Note {
                message: hint.to_string(),
                span: None,
                file_path: None,
            });
        }
    }
}

/// Mutable state threaded through the module tree walk (Visitor pattern).
pub(in crate::driver::per_module) struct TreeWalkState<'a> {
    pub(in crate::driver::per_module) acc: &'a mut ModuleTreeAccumulator,
    pub(in crate::driver::per_module) elab_profile: &'a mut profile::ElabProfile,
}

/// Recursively elaborate modules in post-order (children first).
///
/// Sibling modules are sorted by dependency order (modules that are depended
/// on are processed first) so cross-sibling imports resolve to full definitions
/// rather than stubs.
///
/// When `TUNGSTEN_ELAB_THREADS > 1`, sibling modules at the same dependency
/// level are elaborated in parallel (ADR 11.5.26b §P5).
fn elaborate_module_tree_rec(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(), Vec<ElabError>> {
    if ctx.thread_count > 1 && module.submodules.len() > 1 {
        elaborate_children_parallel(module, module_path, ctx, state)?;
    } else {
        elaborate_children_serial(module, module_path, ctx, state)?;
    }

    // Elaborate this module itself (after all children are done)
    body::elaborate_self(module, module_path, ctx, state)
}

/// Serial child elaboration: topological sort, process one-by-one.
fn elaborate_children_serial(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(), Vec<ElabError>> {
    let sorted_indices = cache::levels::sort_submodules_by_deps(&module.submodules);
    for &idx in &sorted_indices {
        let child_name = modules::get_module_name_from_parsed(&module.submodules[idx]);
        let mut child_path = module_path.to_vec();
        child_path.push(child_name);
        elaborate_module_tree_rec(&module.submodules[idx], &child_path, ctx, state)?;
    }
    Ok(())
}

/// Parallel child elaboration: level-set scheduling with rayon (ADR 11.5.26b §P5).
///
/// Modules at the same dependency level are elaborated concurrently. After each
/// level completes, results are merged into the accumulator in index order for
/// deterministic output. Each worker gets a snapshot of the accumulated exports
/// and its own accumulator.
fn elaborate_children_parallel(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) -> Result<(), Vec<ElabError>> {
    let level_sets = cache::levels::sort_submodules_into_levels(&module.submodules);

    // Use the shared pool from ElabTreeCtx (ADR 11.5.26b §P5).
    // Fallback to serial if pool creation failed at init time.
    let pool = match ctx.parallel_pool.as_ref() {
        Some(p) => p,
        None => return elaborate_children_serial(module, module_path, ctx, state),
    };

    for level in &level_sets {
        if level.len() == 1 {
            // Single module — no parallelism overhead
            let idx = level[0];
            let child_name = modules::get_module_name_from_parsed(&module.submodules[idx]);
            let mut child_path = module_path.to_vec();
            child_path.push(child_name);
            elaborate_module_tree_rec(&module.submodules[idx], &child_path, ctx, state)?;
            continue;
        }

        // Snapshot exports for this level (read-only for workers)
        let exports_snapshot = state.acc.exports.clone();

        // Parallel elaboration of all modules in this level
        let results: Vec<_> = pool.install(|| {
            use rayon::prelude::*;
            level
                .par_iter()
                .map(|&idx| {
                    let child_name = modules::get_module_name_from_parsed(&module.submodules[idx]);
                    let mut child_path = module_path.to_vec();
                    child_path.push(child_name);
                    let mut worker_acc = ModuleTreeAccumulator::new();
                    worker_acc.merge_exports(exports_snapshot.clone());
                    let mut worker_profile = profile::ElabProfile::new();
                    let mut worker_state = TreeWalkState {
                        acc: &mut worker_acc,
                        elab_profile: &mut worker_profile,
                    };
                    let result = elaborate_module_tree_rec(
                        &module.submodules[idx],
                        &child_path,
                        ctx,
                        &mut worker_state,
                    );
                    (idx, result, worker_acc, worker_profile)
                })
                .collect()
        });

        // Merge results in index order for deterministic output
        let mut sorted_results = results;
        sorted_results.sort_by_key(|(idx, _, _, _)| *idx);

        for (_idx, result, worker_acc, worker_profile) in sorted_results {
            result?;
            state.acc.merge_worker(worker_acc);
            state.elab_profile.merge_from(&worker_profile);
        }
    }

    Ok(())
}
