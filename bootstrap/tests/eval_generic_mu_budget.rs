//! Wall-2 minimal eval reproducer (ADR 21.7.26e).
//!
//! Composing generic calls over a μ-type with two recursive occurrences per
//! node (`StrMap`'s k=2 shape) must evaluate in bounded time on the no-LLVM
//! evaluator. Before the ADR 21.7.26e evaluator sweep, the exact shape below
//! — `option_unwrap_or(strmap_lookup(empty_strmap, "k"), 999)` — ran for 47
//! minutes at ~800 MB RSS under `tungsten test`, sampled thousands of frames
//! deep in `Term::strip_spans`, and wedged unkillably (bug report
//! 21.7.26.l1-generic-adt-walls). The single calls (`strmap_lookup` alone,
//! `strmap_height`) always evaluated instantly; the trigger is specifically
//! *generic-call composition*, which this test pins under a wall-clock
//! budget. Elaboration and evaluation both run on a worker thread so a
//! regression fails the test cleanly instead of hanging the suite.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Generous wall-clock ceiling: the fixed evaluator finishes in milliseconds;
/// the pre-fix blow-up ran for 47 minutes. Anything near this bound is broken.
const WALL2_EVAL_BUDGET: Duration = Duration::from_secs(30);

/// The exact bug-report shape: a k=2 generic recursive ADT, a generic
/// producer returning `Option<V>`, and a generic consumer unwrapping it.
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

/// Elaborate `WALL2_SOURCE` and evaluate `def_name`'s body, returning the
/// resulting Nat. Runs entirely on the caller's thread (terms/envs are not
/// `Send`, so the whole pipeline lives inside the worker).
fn elaborate_and_eval_nat(def_name: &str) -> u64 {
    let (ast, parse_errors) = tungsten_bootstrap::parse(WALL2_SOURCE);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");

    let ctx = Box::leak(Box::new(tungsten_core::Context::new()));
    let defs = tungsten_bootstrap::elaborate(&ast, ctx)
        .unwrap_or_else(|errors| panic!("elaboration errors: {errors:?}"));

    let globals: std::collections::HashMap<String, tungsten_core::Term> = defs
        .iter()
        .map(|d| (d.name.clone(), d.term.term.clone()))
        .collect();
    let body = globals
        .get(def_name)
        .unwrap_or_else(|| panic!("definition `{def_name}` not found"))
        .clone();

    // No records and no `compare` in the fixture — an empty RecordTypes map
    // is all the comparator-synthesis callback needs.
    let types = tungsten_bootstrap::comparator::ComparatorTypes::default();
    let env = tungsten_bootstrap::comparator::eval::eval_env(globals, &types);

    let result = tungsten_core::eval::eval_with_env(&body, &env)
        .unwrap_or_else(|stopped| panic!("evaluation stopped without a value: {stopped}"));
    // Nat results come back as either `NatLit` or a Peano `Succ` chain.
    tungsten_core::eval::term_to_nat(&result)
        .unwrap_or_else(|| panic!("expected a Nat result, got a non-Nat term")) as u64
}

/// Run one eval-returning-Nat scenario under the wall-clock budget.
fn assert_evaluates_within_budget(def_name: &'static str, expected: u64) {
    let (result_sender, result_receiver) = mpsc::channel();
    let started = Instant::now();
    // Generous stack: the `999` sentinel elaborates to a depth-1003 Peano
    // `Succ` chain (nat_smart's unary threshold is 1000), and the evaluator's
    // derived recursive traversals (clone/strip_spans) need ~1-2 KB per level
    // in debug builds. A default 2 MB spawn stack overflows mid-clone, which
    // macOS can wedge into an unkillable `UE` thread instead of aborting —
    // the exact zombie mode from the 21.7.26e bug report.
    thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            // The receiver may have given up (budget exceeded) — ignore send errors.
            let _ = result_sender.send(elaborate_and_eval_nat(def_name));
        })
        .expect("failed to spawn eval worker");

    match result_receiver.recv_timeout(WALL2_EVAL_BUDGET) {
        Ok(value) => {
            assert_eq!(value, expected, "{def_name} evaluated to the wrong value");
            // Sanity margin: the fixed evaluator is ~instant; log-worthy drift
            // toward the ceiling would signal the blow-up creeping back.
            assert!(
                started.elapsed() < WALL2_EVAL_BUDGET,
                "{def_name} finished but only just inside the budget"
            );
        }
        Err(_) => panic!(
            "wall-2 blow-up: `{def_name}` exceeded the {WALL2_EVAL_BUDGET:?} \
             evaluator budget (pre-fix this shape ran 47 minutes; \
             see ADR 21.7.26e §2.2)"
        ),
    }
}

#[test]
fn generic_composition_over_k2_mu_type_missing_key() {
    // The exact 47-minute hang shape from the bug report. The present-key /
    // string_compare-extern path is covered natively by
    // src/compiler/test_strmap.tg (`test_strmap_single_insert_hit_and_miss`)
    // under `tungsten test`, where the extern-call FFI environment is real.
    assert_evaluates_within_budget("wall2_lookup_missing_key", 999);
}
