//! Codegen driver loops: compile each unit into its own LLVM module, either
//! sequentially (single `LlvmContext`) or in parallel via bounded work-stealing
//! (ADR 9.5.26e §P3). The parallel driver additionally drains a serial queue
//! on one dedicated worker so pathological multi-GB units never compile
//! concurrently (ADR 3.7.26b Stub Registration). The `__mono` depot is compiled by the
//! caller (`run_per_module_codegen`) after the drivers return.

use std::path::Path;
use std::sync::Mutex;

use tungsten_codegen::inkwell::context::Context as LlvmContext;

use super::unit_compile::{compile_single_unit, OutputConfig};
use super::unit_selection::{UnitSchedule, UnitWork};
use super::{CompiledModule, OutputKind, UnitCompileCtx};

/// Where driver output lands: the shared output directory and whether units
/// emit native objects (`.o`) or LLVM IR text (`.ll`).
#[derive(Clone, Copy)]
pub(super) struct EmitTarget<'a> {
    pub(super) output_dir: &'a Path,
    pub(super) emit_obj: bool,
}

/// Compile the work item at `idx` into its own file with a fresh
/// `LlvmContext`.
///
/// A fresh context per unit is load-bearing (ADR 2.7.26a §3.2): LLVM contexts
/// intern every type and constant for their whole lifetime, so any context
/// shared across units accumulates unbounded memory. The index prefix in the
/// filename prevents case-insensitive filesystem collisions (e.g., char_A.o
/// vs char_a.o on macOS APFS).
fn compile_unit_at(
    idx: usize,
    work: &UnitWork<'_>,
    ctx: &UnitCompileCtx<'_>,
    target: EmitTarget<'_>,
) -> Result<CompiledModule, String> {
    let (ext, kind) = if target.emit_obj {
        ("o", OutputKind::Obj)
    } else {
        ("ll", OutputKind::Ll)
    };
    let output_path = target
        .output_dir
        .join(format!("{}_{}.{}", idx, work.unit_name, ext));

    // Position tag, not a completion counter: i is the unit's stable
    // pre-assigned index (the filename prefix), so under parallel workers
    // the lines need not print in i order (ADR 8.7.26a §2.4).
    let progress_tag = unit_progress_tag(idx, ctx.total_units);
    if ctx.flags.verbose {
        eprintln!("Compiling module '{}'... {}", work.unit_name, progress_tag);
    }

    let output_cfg = OutputConfig {
        path: &output_path,
        emit_obj: target.emit_obj,
    };
    let unit_start = std::time::Instant::now();
    let alloc_start = tungsten_core::diagnostics::alloc_counter::thread_allocated_bytes();
    let llvm_context = LlvmContext::create();
    compile_single_unit(&llvm_context, work, ctx, &output_cfg)?;
    let wall_time_secs = unit_start.elapsed().as_secs_f64();
    // Valid per-unit attribution: the unit compiles wholly on this thread,
    // and the counter is thread-local (ADR 8.7.26a §2.2).
    let alloc_bytes = tungsten_core::diagnostics::alloc_counter::thread_allocated_bytes()
        .wrapping_sub(alloc_start);
    if ctx.flags.verbose {
        // Per-unit census line (ADR 3.7.26b, extended by 8.7.26a): slow units
        // are the multi-GB IR-construction units, and alloc volume separates
        // memory-heavy units that near-equal times hide.
        eprintln!(
            "[perf] unit {}: {:.2}s alloc={} {}",
            work.unit_name,
            wall_time_secs,
            tungsten_core::diagnostics::unit_cost::format_bytes(alloc_bytes),
            progress_tag,
        );
    }
    if let Some(sink) = ctx.unit_cost_sink {
        sink.lock()
            .unwrap()
            .push(tungsten_core::diagnostics::unit_cost::UnitCostRecord {
                unit_name: work.unit_name.clone(),
                unit_index: idx,
                wall_time_secs,
                alloc_bytes,
            });
    }

    Ok(CompiledModule {
        output_path,
        name: work.unit_name.clone(),
        kind,
    })
}

/// `[i/N]` progress tag for verbose stage-1 lines (ADR 8.7.26a §2.4). `i` is
/// the unit's stable pre-assigned index — the same 0-based index used as the
/// output filename prefix — so a census line correlates directly with its
/// emitted `.ll`/`.o` file.
fn unit_progress_tag(idx: usize, total: usize) -> String {
    format!("[{idx}/{total}]")
}

/// Sequential codegen path — no thread overhead (ADR 9.5.26e §P3).
pub(super) fn compile_units_sequential(
    work: &[UnitWork<'_>],
    ctx: &UnitCompileCtx<'_>,
    target: EmitTarget<'_>,
) -> Result<Vec<CompiledModule>, String> {
    let mut compiled = Vec::new();
    for (i, item) in work.iter().enumerate() {
        compiled.push(compile_unit_at(i, item, ctx, target)?);
    }
    Ok(compiled)
}

/// Drain `queue`, running each popped index through `run_item`; results
/// append in pop order.
fn drain_index_queue<T>(
    queue: &Mutex<std::vec::IntoIter<usize>>,
    run_item: &(impl Fn(usize) -> T + Sync),
    out: &mut Vec<T>,
) {
    loop {
        let item = { queue.lock().unwrap().next() };
        let Some(idx) = item else { break };
        out.push(run_item(idx));
    }
}

/// Generic worker-pool core of the parallel driver (ADR 3.7.26b Stub Registration):
/// `schedule.serial` indices are drained in order by worker 0 ONLY — at most
/// one of them is ever in flight — while `schedule.parallel` indices
/// work-steal across all workers; worker 0 joins the pool once its serial
/// queue is empty. Split from [`compile_units_parallel`] so the
/// serialization property is testable with a probe closure instead of real
/// LLVM codegen (see `serial_units_never_run_concurrently`).
fn run_scheduled_workers<T: Send>(
    schedule: UnitSchedule,
    worker_count: usize,
    run_item: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    let serial_queue: Mutex<std::vec::IntoIter<usize>> = Mutex::new(schedule.serial.into_iter());
    let parallel_queue: Mutex<std::vec::IntoIter<usize>> =
        Mutex::new(schedule.parallel.into_iter());

    std::thread::scope(|s| {
        let handles: Vec<_> = (0..worker_count.max(1))
            .map(|worker_id| {
                let serial_queue = &serial_queue;
                let parallel_queue = &parallel_queue;
                let run_item = &run_item;
                s.spawn(move || {
                    let mut local_results = Vec::new();
                    if worker_id == 0 {
                        drain_index_queue(serial_queue, run_item, &mut local_results);
                    }
                    drain_index_queue(parallel_queue, run_item, &mut local_results);
                    local_results
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

/// Parallel codegen path — bounded work-stealing via std::thread::scope
/// (ADR 9.5.26e §P3), with serial-unit mitigation (ADR 3.7.26b Stub Registration) via
/// [`run_scheduled_workers`]. Total worker count never exceeds
/// `codegen_jobs` — the mitigation may only lower effective parallelism.
pub(super) fn compile_units_parallel(
    work: &[UnitWork<'_>],
    ctx: &UnitCompileCtx<'_>,
    target: EmitTarget<'_>,
    codegen_jobs: usize,
    schedule: UnitSchedule,
) -> Result<Vec<CompiledModule>, String> {
    let worker_count = codegen_jobs.min(work.len()).max(1);
    let results: Vec<Result<CompiledModule, String>> =
        run_scheduled_workers(schedule, worker_count, |idx| {
            compile_unit_at(idx, &work[idx], ctx, target)
        });

    // Collect results, propagating errors
    let mut compiled = Vec::with_capacity(results.len());
    let mut errors = Vec::new();
    for r in results {
        match r {
            Ok(m) => compiled.push(m),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    // Sort for deterministic link order regardless of completion order
    compiled.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The progress tag shows the stable filename-prefix index, not a
    /// completion count — `[0/2030]` is unit 0 of 2030, matching `0_<unit>.ll`.
    #[test]
    fn progress_tag_is_index_slash_total() {
        assert_eq!(unit_progress_tag(0, 2030), "[0/2030]");
        assert_eq!(unit_progress_tag(716, 2030), "[716/2030]");
    }

    /// The ADR 3.7.26b Stub Registration memory guarantee: serial-scheduled indices
    /// are NEVER in flight concurrently, even with a large worker pool.
    /// Structural, not timing-dependent — only worker 0 pops the serial
    /// queue — so the sleep makes a violation observable without ever
    /// making a correct implementation flake.
    #[test]
    fn serial_units_never_run_concurrently() {
        let schedule = UnitSchedule {
            serial: vec![0, 2, 4],
            parallel: vec![1, 3, 5, 6, 7],
        };
        let serial_set: HashSet<usize> = schedule.serial.iter().copied().collect();
        let serial_in_flight = AtomicUsize::new(0);
        let max_serial_in_flight = AtomicUsize::new(0);

        let results = run_scheduled_workers(schedule, 4, |idx| {
            if serial_set.contains(&idx) {
                let now = serial_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_serial_in_flight.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(10));
                serial_in_flight.fetch_sub(1, Ordering::SeqCst);
            }
            idx
        });

        assert_eq!(
            max_serial_in_flight.load(Ordering::SeqCst),
            1,
            "serial units overlapped — the OOM mitigation guarantee is broken"
        );
        let mut ran: Vec<usize> = results;
        ran.sort_unstable();
        assert_eq!(
            ran,
            (0..8).collect::<Vec<_>>(),
            "every index ran exactly once"
        );
    }

    /// Serial indices complete in schedule order (worker 0 drains the queue
    /// front-to-back), and all run before worker 0 joins the parallel pool.
    #[test]
    fn serial_units_complete_in_schedule_order() {
        let schedule = UnitSchedule {
            serial: vec![3, 1, 5],
            parallel: vec![0, 2, 4],
        };
        let completion_log: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        run_scheduled_workers(schedule, 2, |idx| {
            completion_log.lock().unwrap().push(idx);
        });
        let mut serial_order = completion_log.into_inner().unwrap();
        serial_order.retain(|idx| [3, 1, 5].contains(idx));
        assert_eq!(serial_order, vec![3, 1, 5]);
    }

    /// A single worker still drains both queues to completion (jobs=1 via
    /// the parallel path, e.g. when units.len() == 1 caps the pool).
    #[test]
    fn single_worker_drains_serial_then_parallel() {
        let schedule = UnitSchedule {
            serial: vec![1],
            parallel: vec![0, 2],
        };
        let mut ran: Vec<usize> = run_scheduled_workers(schedule, 1, |idx| idx);
        assert_eq!(ran.remove(0), 1, "serial queue drains first on worker 0");
        ran.sort_unstable();
        assert_eq!(ran, vec![0, 2]);
    }
}
