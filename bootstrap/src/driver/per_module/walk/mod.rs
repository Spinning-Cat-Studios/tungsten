//! The module-tree walk: post-order traversal, serial and parallel.
//!
//! Extracted from `per_module/mod.rs` (ADR 14.8.26g) so the walk's error
//! accumulation lives beside the traversal it governs.
//!
//! The walk **accumulates and keeps going** (ADR 14.8.26g D1). It used to
//! `?` out at the first module whose Body Elaboration failed — the compiler's
//! dominant error short-circuit (ADR 7.8.26d P1): one broken module hid every
//! dependent module's diagnostics, and cascade suppression upstream was inert
//! because no dependent was ever reached. Now a failing module's errors are
//! recorded in the accumulator (`record_module_errors`) and the walk
//! continues; the run fails at the end, in `run_body_elaboration`.

pub(super) mod body;

use crate::elaborate::ElabError;

use super::accumulator::ModuleTreeAccumulator;
use super::modules::{self, ParsedModule};
use super::{cache, profile, ElabTreeCtx};

/// Mutable state threaded through the module tree walk (Visitor pattern).
pub(in crate::driver::per_module) struct TreeWalkState<'a> {
    pub(in crate::driver::per_module) acc: &'a mut ModuleTreeAccumulator,
    pub(in crate::driver::per_module) elab_profile: &'a mut profile::ElabProfile,
}

/// The canonical order module error groups are reported in: the serial
/// walker's own traversal — children in dependency-sorted order, then self.
///
/// Computed from the tree rather than recorded during the walk so the
/// parallel walker (whose per-level merge order differs) reports the same
/// list byte-for-byte (ADR 14.8.26g D1: diagnostics must not depend on
/// `thread_count`).
pub(super) fn canonical_walk_order(
    module: &ParsedModule,
    module_path: &[String],
    out: &mut Vec<Vec<String>>,
) {
    let sorted_indices = cache::levels::sort_submodules_by_deps(&module.submodules);
    for &idx in &sorted_indices {
        let child_name = modules::get_module_name_from_parsed(&module.submodules[idx]);
        let mut child_path = module_path.to_vec();
        child_path.push(child_name);
        canonical_walk_order(&module.submodules[idx], &child_path, out);
    }
    out.push(module_path.to_vec());
}

/// Recursively elaborate modules in post-order (children first).
///
/// Sibling modules are sorted by dependency order (modules that are depended
/// on are processed first) so cross-sibling imports resolve to full definitions
/// rather than stubs.
///
/// When `TUNGSTEN_ELAB_THREADS > 1`, sibling modules at the same dependency
/// level are elaborated in parallel (ADR 11.5.26b §P5).
///
/// A failing module records its errors and the walk continues (ADR 14.8.26g
/// D1) — this function has no failure return; the run's verdict is read from
/// the accumulator after the walk.
pub(super) fn elaborate_module_tree_rec(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) {
    if module_failure_budget_reached(ctx.module_failure_budget, state.acc.module_errors.len()) {
        return;
    }

    if should_elaborate_children_in_parallel(ctx.thread_count, module.submodules.len()) {
        elaborate_children_parallel(module, module_path, ctx, state);
    } else {
        elaborate_children_serial(module, module_path, ctx, state);
    }

    // Elaborate this module itself (after all children are done)
    if let Err(errors) = body::elaborate_self(module, module_path, ctx, state) {
        record_module_errors(state, module_path, errors);
    }
}

/// Whether the walk has met its module bail-out (ADR 14.8.26g D6's
/// wall-clock arm): once it has accumulated a display budget's worth of
/// *failing modules*, walking on costs wall clock and gains nothing
/// displayable — D7's truncation could not render it. Skipped modules stay
/// uncounted, so the unreached-module note reports what the bail-out left
/// unexamined.
///
/// A budget of 0 (`--max-errors=0`, the measurement setting) never bails.
/// In the parallel walker each worker sees only its own accumulator
/// mid-level, so the bail-out lands at the next level boundary rather than
/// mid-level; the *bailed* regime's note may therefore vary with
/// `thread_count`, while the exhaustive regime's diagnostics do not.
///
/// Split from the walker (retrospective on ADR 14.8.26g) so this predicate —
/// which gates a user-visible budget and is pinned by
/// `the_module_bail_out_stops_at_the_display_budget` — does not share a
/// function with the scheduling choice below, whose comparisons are
/// deliberately result-invisible and therefore mutation-allowlisted.
pub(super) fn module_failure_budget_reached(budget: usize, failing_modules: usize) -> bool {
    budget > 0 && failing_modules >= budget
}

/// Whether this level's children go to the parallel walker (ADR 11.5.26b §P5).
///
/// **Purely a scheduling choice**: both walkers produce byte-identical
/// diagnostics and accumulator state by design (ADR 14.8.26g D1), so a wrong
/// answer here is invisible in every result — which is why this function's
/// comparisons are the ones on the mutation-survivor allowlist, and why they
/// live apart from the budget predicate above.
pub(super) fn should_elaborate_children_in_parallel(
    thread_count: usize,
    submodule_count: usize,
) -> bool {
    thread_count > 1 && submodule_count > 1
}

/// Record a failed module's errors against its path, so the final report can
/// order the groups canonically whatever order the walk produced them in.
fn record_module_errors(
    state: &mut TreeWalkState<'_>,
    module_path: &[String],
    errors: Vec<ElabError>,
) {
    state.acc.module_errors.push((module_path.to_vec(), errors));
}

/// Serial child elaboration: topological sort, process one-by-one.
///
/// A failing sibling no longer ends the level (ADR 14.8.26g D1).
fn elaborate_children_serial(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) {
    let sorted_indices = cache::levels::sort_submodules_by_deps(&module.submodules);
    for &idx in &sorted_indices {
        let child_name = modules::get_module_name_from_parsed(&module.submodules[idx]);
        let mut child_path = module_path.to_vec();
        child_path.push(child_name);
        elaborate_module_tree_rec(&module.submodules[idx], &child_path, ctx, state);
    }
}

/// Parallel child elaboration: level-set scheduling with rayon (ADR 11.5.26b §P5).
///
/// Modules at the same dependency level are elaborated concurrently. After each
/// level completes, results are merged into the accumulator in index order for
/// deterministic output. Each worker gets a snapshot of the accumulated exports
/// and its own accumulator; a worker's errors ride in its accumulator's
/// `module_errors` and are merged like any other result (ADR 14.8.26g D1).
fn elaborate_children_parallel(
    module: &ParsedModule,
    module_path: &[String],
    ctx: &ElabTreeCtx<'_>,
    state: &mut TreeWalkState<'_>,
) {
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
            elaborate_module_tree_rec(&module.submodules[idx], &child_path, ctx, state);
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
                    elaborate_module_tree_rec(
                        &module.submodules[idx],
                        &child_path,
                        ctx,
                        &mut worker_state,
                    );
                    (idx, worker_acc, worker_profile)
                })
                .collect()
        });

        // Merge results in index order for deterministic output
        let mut sorted_results = results;
        sorted_results.sort_by_key(|(idx, _, _)| *idx);

        for (_idx, worker_acc, worker_profile) in sorted_results {
            state.acc.merge_worker(worker_acc);
            state.elab_profile.merge_from(&worker_profile);
        }
    }
}
