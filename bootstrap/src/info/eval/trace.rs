//! `tungsten info eval trace <def> <file>` — evaluator step tracer (ADR 21.7.26j).
//!
//! Productizes the ad-hoc step-shape probe hand-built during the 21.7.26e
//! wall-2 investigation. For each small-step reduction of a definition's body
//! it prints one line — step index, a **bounded** node count, and a
//! depth-limited shape rendering — so "is it looping, growing, or stuck?" is
//! answerable at a glance instead of from RSS graphs and `sample` traces.
//!
//! The loop runs on a wide-stack worker thread: the evaluator can recurse
//! deeply on a pathological term, and a default stack wedges into an
//! unkillable macOS `UE` mid-overflow (the 21.7.26e deep-Peano lesson) — the
//! tracer must never reproduce the failure mode it exists to diagnose.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::comparator::eval::eval_env;
use tungsten_bootstrap::comparator::ComparatorTypes;
use tungsten_bootstrap::driver::{self};
use tungsten_core::diagnostics::term_shape;
use tungsten_core::eval::{step_with_env, EvalEnv, StepResult};
use tungsten_core::Term;

/// Worker stack size for the trace loop. 512 MiB mirrors the headroom the
/// compiled binary requests via the linker `-stack_size` flag, giving the
/// stepper room to recurse on a deep term without overflowing.
const TRACE_WORKER_STACK_BYTES: usize = 512 * 1024 * 1024;

/// Tunables for a single trace run, mirroring the CLI flags.
pub struct TraceOptions {
    /// Stop after this many reduction steps (default 1000).
    pub max_steps: usize,
    /// Levels of structure to render per step (default 2).
    pub shape_depth: usize,
    /// Node-counting budget per step; a term larger than this reports `≥M`
    /// rather than being fully walked (default `100_000`).
    pub limit_nodes: usize,
}

/// Terminal state a trace run reached.
#[derive(Debug, PartialEq, Eq)]
enum TraceEnd {
    /// The term reduced to a value.
    Value,
    /// The term got stuck (open term, unresolved FFI, or a black-holed global).
    Stuck,
    /// `max_steps` elapsed while the term was still reducing.
    MaxSteps,
}

/// Trace the evaluator step-by-step for `name` in `file`.
///
/// Elaborates the (multi-module-capable) project, builds the same globals map +
/// `EvalEnv` that `tungsten test` uses, then drives `step_with_env` on a
/// wide-stack worker. Read-only; exit is `SUCCESS` for any completed trace,
/// `FAILURE` only when elaboration fails or the definition is absent.
pub fn cmd_info_eval_trace(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    opts: &TraceOptions,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let Some(def) = project.defs.iter().find(|d| d.name == name) else {
        eprintln!("error: no definition named '{name}' in {}", file.display());
        return ExitCode::FAILURE;
    };

    // Span-stripped body — the same normalization the evaluator entry points do
    // before stepping (spans are noise in the shape rendering).
    let term = def.term.term.strip_spans();

    // Globals for environment-based evaluation: every def's body, exactly the
    // map `tungsten test`/`tungsten run` build so cross-module helpers resolve.
    let globals: HashMap<String, Term> = project
        .defs
        .iter()
        .map(|d| (d.name.clone(), d.term.term.clone()))
        .collect();
    let comparator_types = ComparatorTypes::new(
        project.record_types.clone(),
        &project.encoded_types,
        &project.type_provenance,
        project.adt_types.clone(),
        &project.mutual_recursion_groups,
    );

    println!(
        "Tracing '{name}' (max-steps={}, shape-depth={}, limit-nodes={}):",
        opts.max_steps, opts.shape_depth, opts.limit_nodes
    );
    println!();

    let (end, steps) = run_trace_on_worker(term, globals, comparator_types, opts);

    match end {
        TraceEnd::Value => println!("VALUE after {steps} steps"),
        TraceEnd::Stuck => println!("STUCK after {steps} steps (open term / FFI / black hole)"),
        TraceEnd::MaxSteps => println!(
            "MAX STEPS ({}) reached — term still reducing (raise --max-steps to see more)",
            opts.max_steps
        ),
    }

    ExitCode::SUCCESS
}

/// Run [`drive_trace`] on a wide-stack worker thread and join the result.
///
/// The `EvalEnv` carries `Rc` (its comparator-synthesis callback), so it is
/// built *inside* the worker; only `Send` data (`globals`, `types`,
/// `term`) crosses the thread boundary.
fn run_trace_on_worker(
    term: Term,
    globals: HashMap<String, Term>,
    types: ComparatorTypes,
    opts: &TraceOptions,
) -> (TraceEnd, usize) {
    let (max_steps, shape_depth, limit_nodes) =
        (opts.max_steps, opts.shape_depth, opts.limit_nodes);
    let handle = std::thread::Builder::new()
        .name("eval-trace".to_string())
        .stack_size(TRACE_WORKER_STACK_BYTES)
        .spawn(move || {
            let env = eval_env(globals, &types);
            drive_trace(&term, &env, max_steps, shape_depth, limit_nodes)
        })
        .expect("failed to spawn eval-trace worker thread");
    handle.join().expect("eval-trace worker thread panicked")
}

/// The step loop: print the current term, take one `step_with_env`, repeat
/// until `Value`/`Stuck`/`max_steps`. Returns the terminal state and the number
/// of reduction steps taken.
fn drive_trace(
    term: &Term,
    env: &EvalEnv,
    max_steps: usize,
    shape_depth: usize,
    limit_nodes: usize,
) -> (TraceEnd, usize) {
    let mut current = term.clone();
    let mut step = 0usize;
    let end = loop {
        println!(
            "{}",
            format_step_line(step, &current, shape_depth, limit_nodes)
        );
        match step_with_env(&current, env) {
            StepResult::Value => break TraceEnd::Value,
            StepResult::Stuck => break TraceEnd::Stuck,
            StepResult::Stepped(next) => {
                if step >= max_steps {
                    break TraceEnd::MaxSteps;
                }
                current = next;
                step += 1;
            }
        }
    };
    (end, step)
}

/// Format one trace line: `step   N  nodes   M  <shape>` (pure — unit-tested).
fn format_step_line(step: usize, term: &Term, shape_depth: usize, limit_nodes: usize) -> String {
    let nodes = term_shape::count_nodes_bounded(term, limit_nodes);
    let shape = term_shape::render_shape(term, shape_depth);
    format!("step {:>4}  nodes {:>8}  {}", step, nodes.render(), shape)
}

// Tests: trace_tests.rs
#[cfg(test)]
#[path = "trace_tests.rs"]
mod trace_tests;
