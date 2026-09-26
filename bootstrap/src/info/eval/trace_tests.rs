//! Tests for the evaluator step tracer (ADR 21.7.26j).
//!
//! Covers the three terminal states (`Value` / `Stuck` / `MaxSteps`) through
//! the wide-stack worker, and end-to-end traces the 21.7.26e wall-2 reproducer
//! — `option_unwrap_or(strmap_lookup(empty_strmap, "k"), 999)` — proving it
//! reaches a value in the twelve small steps the hand-built probe observed.

use super::*;
use std::collections::HashMap;
use tungsten_core::types::Type;
use tungsten_core::Term;

/// Default-shaped options with a caller-chosen `max_steps`.
fn opts(max_steps: usize) -> TraceOptions {
    TraceOptions {
        max_steps,
        shape_depth: 2,
        limit_nodes: 100_000,
    }
}

#[test]
fn trace_reaches_value_on_a_reducible_term() {
    // NatAdd(1, 2) reduces to the value 3 in one step.
    let term = Term::NatAdd(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
    let (end, steps) =
        run_trace_on_worker(term, HashMap::new(), ComparatorTypes::default(), &opts(100));
    assert_eq!(end, TraceEnd::Value);
    assert_eq!(steps, 1, "1 + 2 folds to NatLit(3) in a single step");
}

#[test]
fn trace_reports_stuck_on_an_open_term() {
    // A bare Var is an open term — stuck immediately, zero steps taken.
    let (end, steps) = run_trace_on_worker(
        Term::var("x"),
        HashMap::new(),
        ComparatorTypes::default(),
        &opts(100),
    );
    assert_eq!(end, TraceEnd::Stuck);
    assert_eq!(steps, 0);
}

#[test]
fn trace_caps_a_diverging_term_at_max_steps() {
    // `fix f. f` unfolds to itself forever; the tracer must stop at the cap,
    // never reproduce the non-termination it exists to diagnose.
    let term = Term::fix("f", Type::Nat, Term::var("f"));
    let (end, steps) =
        run_trace_on_worker(term, HashMap::new(), ComparatorTypes::default(), &opts(20));
    assert_eq!(end, TraceEnd::MaxSteps);
    assert_eq!(steps, 20);
}

/// The exact 21.7.26e wall-2 shape: a k=2 generic recursive ADT, a generic
/// producer returning `Option<V>`, and a generic consumer unwrapping it. The
/// `999` default elaborates to a ~1000-node Peano `Succ` chain — the value the
/// trace terminates on.
const WALL2_SOURCE: &str = r#"
extern "C" fn tg_string_compare(a: String, b: String) -> Nat

fn string_compare(a: String, b: String) -> Nat {
    tg_string_compare(a, b)
}

type Option<T> = None | Some(T)
type StrMap<V> = StrMapLeaf | StrMapNode(StrMap<V>, String, V, StrMap<V>, Nat)

fn option_unwrap_or<T>(opt: Option<T>, default: T) -> T {
    match opt {
        Some(value) => value,
        None() => default
    }
}

fn strmap_lookup<V>(m: StrMap<V>, key: String) -> Option<V> {
    match m {
        StrMapLeaf() => None,
        StrMapNode(left, node_key, value, right, _) =>
            let cmp = string_compare(key, node_key);
            if cmp == 0 {
                strmap_lookup(left, key)
            } else if cmp == 2 {
                strmap_lookup(right, key)
            } else {
                Some(value)
            }
    }
}

fn wall2_lookup_missing_key() -> Nat {
    let empty: StrMap<Nat> = StrMapLeaf;
    option_unwrap_or(strmap_lookup(empty, "k"), 999)
}
"#;

#[test]
fn wall2_reproducer_traces_to_value_in_twelve_steps() {
    // Elaborate + trace on a generous stack: the `999` sentinel is a
    // depth-~1000 Peano chain, and both elaboration and the stepper recurse
    // over it (the 21.7.26e unkillable-`UE` mode is exactly a stack overflow
    // on this shape under a default 2 MB test thread).
    let (end, steps) = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let (ast, parse_errors) = tungsten_bootstrap::parse(WALL2_SOURCE);
            assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
            let mut ctx = tungsten_core::Context::new();
            let defs = tungsten_bootstrap::elaborate(&ast, &mut ctx)
                .unwrap_or_else(|errors| panic!("elaboration errors: {errors:?}"));
            let globals: HashMap<String, Term> = defs
                .iter()
                .map(|d| (d.name.clone(), d.term.term.clone()))
                .collect();
            let body = globals
                .get("wall2_lookup_missing_key")
                .expect("wall2_lookup_missing_key must elaborate")
                .clone();
            let types = ComparatorTypes::default();
            let env = eval_env(globals, &types);
            drive_trace(&body, &env, 1000, 2, 100_000)
        })
        .expect("failed to spawn wall-2 trace worker")
        .join()
        .expect("wall-2 trace worker panicked");

    assert_eq!(
        end,
        TraceEnd::Value,
        "the wall-2 composition must reduce to a value, not hang or get stuck"
    );
    assert_eq!(
        steps, 12,
        "the 21.7.26e wall-2 shape reduces in 12 small steps (the hand-built probe's count)"
    );
}

#[test]
fn format_step_line_renders_index_nodes_and_shape() {
    // App(Zero, NatLit(1)): 3 nodes, depth-2 shape "App(Zero, NatLit)".
    let term = Term::app(Term::Zero, Term::NatLit(1));
    let line = format_step_line(7, &term, 2, 1000);
    assert!(line.contains("step "), "line: {line}");
    assert!(line.contains('7'), "step index present: {line}");
    assert!(line.contains('3'), "node count present: {line}");
    assert!(line.contains("App(Zero, NatLit)"), "shape present: {line}");
}

/// End-to-end `cmd_info_eval_trace`: a reducible `main` traces to a value and
/// the command reports success (covers elaboration → def lookup → the Value
/// print arm → wide-stack worker).
#[test]
fn cmd_eval_trace_traces_a_value_and_reports_success() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(&path, "fn main() -> Nat { 1 + 2 }\n").unwrap();
    let opts = TraceOptions {
        max_steps: 50,
        shape_depth: 2,
        limit_nodes: 1000,
    };
    assert_eq!(
        cmd_info_eval_trace("main", &path, false, 20, &opts),
        ExitCode::SUCCESS
    );
}

/// A self-referential global is a black hole: the stepper reports `Stuck`, the
/// command still completes the trace (covers the Stuck print arm).
#[test]
fn cmd_eval_trace_self_reference_reports_stuck_but_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(
        &path,
        // `#[partial]` because the fixture is *deliberately* non-terminating —
        // that is the thing being traced. Since ADR 11.8.26b made enforcement
        // `all` the default, elaboration would otherwise reject it before the
        // tracer ever ran.
        "#[partial]\nfn spin() -> Nat { spin() }\nfn main() -> Nat { 0 }\n",
    )
    .unwrap();
    let opts = TraceOptions {
        max_steps: 50,
        shape_depth: 2,
        limit_nodes: 1000,
    };
    assert_eq!(
        cmd_info_eval_trace("spin", &path, false, 20, &opts),
        ExitCode::SUCCESS
    );
}

/// An absent definition is a failure (covers the def-not-found error arm).
#[test]
fn cmd_eval_trace_missing_def_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(&path, "fn main() -> Nat { 0 }\n").unwrap();
    let opts = TraceOptions {
        max_steps: 50,
        shape_depth: 2,
        limit_nodes: 1000,
    };
    assert_eq!(
        cmd_info_eval_trace("does_not_exist", &path, false, 20, &opts),
        ExitCode::FAILURE
    );
}

/// An unparseable file fails at elaboration (covers the elaboration-error arm).
#[test]
fn cmd_eval_trace_unparseable_file_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(&path, "fn main( -> Nat { \n").unwrap();
    let opts = TraceOptions {
        max_steps: 50,
        shape_depth: 2,
        limit_nodes: 1000,
    };
    assert_eq!(
        cmd_info_eval_trace("main", &path, false, 20, &opts),
        ExitCode::FAILURE
    );
}
