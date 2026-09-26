//! Tests for environment-based evaluation.
//!
//! The comparator-resolution and assertion-boundary cases live in
//! [`comparator_gate`], which is a separate file both for the size limit and
//! because they assert a different property: not "what does this term reduce
//! to" but "what happens when a comparison cannot run at all".

mod comparator_gate;

use std::collections::HashMap;

use crate::eval::env::*;
use crate::types::Type;

#[test]
fn test_eval_env_empty() {
    let env = EvalEnv::empty();
    assert_eq!(env.lookup("foo"), GlobalLookup::Unbound);
}

#[test]
fn extern_call_executes_test_assert_ffi() {
    // A test-assertion FFI runs and steps to Unit (no longer stuck).
    // Use 0 == 0 so the shared thread-local failure flag is untouched.
    let env = EvalEnv::empty();
    let t = Term::ExternCall(
        "__c_tg_assert_eq_bool".to_string(),
        vec![Term::Zero, Term::Zero],
    );
    assert_eq!(eval_with_env(&t, &env), Ok(Term::Unit));
}

#[test]
fn extern_call_check_failure_returns_nat() {
    // `tg_test_check_failure` returns a Nat (value depends on thread-local flag).
    let env = EvalEnv::empty();
    let t = Term::ExternCall("__c_tg_test_check_failure".to_string(), vec![]);
    let r = eval_with_env(&t, &env).expect("no black hole here");
    assert!(matches!(r, Term::Zero | Term::Succ(_) | Term::NatLit(_)));
}

#[test]
fn extern_call_string_compare_three_way() {
    // tg_string_compare evaluates purely: 0 = LT, 1 = EQ, 2 = GT (ADR 21.7.26c).
    let env = EvalEnv::empty();
    let cmp = |a: &str, b: &str| {
        let t = Term::ExternCall(
            "__c_tg_string_compare".to_string(),
            vec![
                Term::StringLit(a.to_string()),
                Term::StringLit(b.to_string()),
            ],
        );
        crate::eval::term_to_nat(&eval_with_env(&t, &env).expect("no black hole here"))
    };
    assert_eq!(cmp("abc", "abd"), Some(0));
    assert_eq!(cmp("abc", "abc"), Some(1));
    assert_eq!(cmp("abd", "abc"), Some(2));
    assert_eq!(cmp("", "a"), Some(0));
}

#[test]
fn extern_call_string_compare_non_string_args_stuck() {
    // Wrong operand shapes stay stuck rather than guessing.
    let env = EvalEnv::empty();
    let t = Term::ExternCall(
        "__c_tg_string_compare".to_string(),
        vec![Term::Zero, Term::Zero],
    );
    assert_eq!(eval_with_env(&t, &env), Ok(t.clone()));
}

#[test]
fn extern_call_unknown_ffi_is_stuck() {
    // Non-test extern calls remain stuck (pure evaluation).
    let env = EvalEnv::empty();
    let t = Term::ExternCall("__c_not_a_test_ffi".to_string(), vec![]);
    assert_eq!(eval_with_env(&t, &env), Ok(t.clone()));
}

#[test]
fn tyapp_substitutes_type_arg_into_body() {
    // `ΛT. λx:T. x` applied to `Nat` substitutes T:=Nat so the lambda's annotation
    // is concrete (required so `__cmp<T>` sees the concrete type).
    let body = Term::ty_abs(
        "T",
        Term::lambda("x", Type::TyVar("T".into()), Term::var("x")),
    );
    let applied = Term::ty_app(body, Type::Nat);
    let stepped = eval_with_env(&applied, &EvalEnv::empty()).expect("no black hole here");
    assert_eq!(
        stepped,
        Term::lambda("x", Type::Nat, Term::var("x")),
        "type arg should be substituted, not erased"
    );
}

#[test]
fn test_eval_env_lookup() {
    let mut globals = HashMap::new();
    globals.insert("x".to_string(), Term::Zero);
    let env = EvalEnv::new(globals);

    assert_eq!(env.lookup("x"), GlobalLookup::Value(Term::Zero));
    assert_eq!(env.lookup("y"), GlobalLookup::Unbound);
}

#[test]
fn test_global_lookup() {
    let mut globals = HashMap::new();
    globals.insert("x".to_string(), Term::Zero);
    let env = EvalEnv::new(globals);

    let result = eval_with_env(&Term::Global("x".into()), &env);
    assert_eq!(result, Ok(Term::Zero));
}

#[test]
fn test_global_undefined_stuck() {
    let env = EvalEnv::empty();
    let result = step_with_env(&Term::Global("undefined".into()), &env);
    assert_eq!(result, StepResult::Stuck);
}

#[test]
fn test_call_by_need_memoization() {
    // Create an environment where looking up "x" returns an expression
    // that requires evaluation
    let mut globals = HashMap::new();
    globals.insert(
        "x".to_string(),
        Term::app(Term::lambda("y", Type::Nat, Term::var("y")), Term::Zero),
    );
    let env = EvalEnv::new(globals);

    // First lookup should evaluate and cache
    let result1 = env.lookup("x");
    assert_eq!(result1, GlobalLookup::Value(Term::Zero));

    // Second lookup should return cached value
    let result2 = env.lookup("x");
    assert_eq!(result2, GlobalLookup::Value(Term::Zero));

    // Verify it's actually cached
    assert!(env.cache.borrow().contains_key("x"));
}

#[test]
fn test_nested_global_references() {
    // x = zero
    // y = x
    // main = y
    let mut globals = HashMap::new();
    globals.insert("x".to_string(), Term::Zero);
    globals.insert("y".to_string(), Term::Global("x".into()));
    globals.insert("main".to_string(), Term::Global("y".into()));
    let env = EvalEnv::new(globals);

    let result = eval_with_env(&Term::Global("main".into()), &env);
    assert_eq!(result, Ok(Term::Zero));
}

#[test]
fn test_global_in_application() {
    // id = λx:Nat. x
    // main = id zero
    let mut globals = HashMap::new();
    globals.insert(
        "id".to_string(),
        Term::lambda("x", Type::Nat, Term::var("x")),
    );
    let env = EvalEnv::new(globals);

    let term = Term::app(Term::Global("id".into()), Term::Zero);
    let result = eval_with_env(&term, &env);
    assert_eq!(result, Ok(Term::Zero));
}

#[test]
fn test_eval_with_env_and_limit_terminates() {
    let env = EvalEnv::empty();
    let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
    let result = eval_with_env_and_limit(&term, &env, 100);
    assert_eq!(result, Ok(Term::Zero));
}

/// A term that steps forever: `fix f. f` unfolds to itself on every step.
/// This is the shape a non-terminating `test_*` body reduces to, and the
/// watchdog's reason for existing (ADR 21.7.26f / D1).
fn diverging_term() -> Term {
    Term::fix("f", Type::Nat, Term::var("f"))
}

#[test]
fn eval_until_returns_value_when_deadline_is_not_reached() {
    let env = EvalEnv::empty();
    let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_eq!(eval_with_env_until(&term, &env, deadline), Ok(Term::Zero));
}

#[test]
fn eval_until_trips_on_a_diverging_term() {
    let env = EvalEnv::empty();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
    let result = eval_with_env_until(&diverging_term(), &env, deadline);
    let stopped = result.expect_err("`fix f. f` never reaches a value — must trip the deadline");
    let EvalStopped::TimedOut { steps } = stopped else {
        panic!("a diverging (non-black-hole) term must trip as TimedOut, got {stopped:?}");
    };
    assert!(
        steps > 0,
        "the trip must report the steps it reached, for the TIMEOUT forensics line"
    );
    assert_eq!(
        steps % super::deadline::DEADLINE_CHECK_INTERVAL,
        0,
        "the deadline is only observed on interval boundaries"
    );
}

#[test]
fn eval_until_trips_after_the_deadline_not_before() {
    let env = EvalEnv::empty();
    let start = std::time::Instant::now();
    let budget = std::time::Duration::from_millis(100);
    let result = eval_with_env_until(&diverging_term(), &env, start + budget);
    assert!(result.is_err());
    assert!(
        start.elapsed() >= budget,
        "tripped before the deadline elapsed ({:?} < {budget:?})",
        start.elapsed()
    );
}

#[test]
fn eval_until_agrees_with_unbounded_eval_below_the_deadline() {
    // The watchdog bounds evaluation; it must not change its results.
    let env = EvalEnv::empty();
    let term = Term::nat_add(Term::NatLit(2), Term::NatLit(3));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_eq!(
        eval_with_env_until(&term, &env, deadline),
        eval_with_env(&term, &env)
    );
}

#[test]
fn eval_until_returns_stuck_terms_rather_than_spinning() {
    // A stuck term is a normal (Ok) outcome, not a timeout.
    let env = EvalEnv::empty();
    let term = Term::var("unbound");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    assert_eq!(eval_with_env_until(&term, &env, deadline), Ok(term.clone()));
}

#[test]
fn test_step_with_env_basic() {
    let env = EvalEnv::empty();

    // Lambda is a value
    assert_eq!(
        step_with_env(&Term::lambda("x", Type::Nat, Term::var("x")), &env),
        StepResult::Value
    );

    // Zero is a value
    assert_eq!(step_with_env(&Term::Zero, &env), StepResult::Value);

    // Variable is stuck
    assert_eq!(step_with_env(&Term::var("x"), &env), StepResult::Stuck);
}

// --- Bug #1 regression: Let body must use named vars, not de Bruijn indices ---

#[test]
fn test_let_named_var_substitutes() {
    // let greeting = Zero in succ(greeting) → succ(Zero)
    let env = EvalEnv::empty();
    let term = Term::let_in(
        "greeting",
        Type::Nat,
        Term::Zero,
        Term::succ(Term::var("greeting")),
    );
    let result = eval_with_env(&term, &env);
    assert_eq!(result, Ok(Term::succ(Term::Zero)));
}

#[test]
fn test_let_debruijn_var_does_not_substitute() {
    // Bug #1 scenario: "$0" doesn't match binder "greeting", so substitution fails
    let env = EvalEnv::empty();
    let term = Term::let_in(
        "greeting",
        Type::Nat,
        Term::Zero,
        Term::succ(Term::var("$0")),
    );
    let result = eval_with_env(&term, &env);
    // Substitution fails → "$0" stays stuck, result is NOT succ(Zero)
    assert_ne!(result, Ok(Term::succ(Term::Zero)));
}

// --- Bug #2 regression: Nested let chains must produce Core Let terms ---

#[test]
fn test_nested_let_chain() {
    // let a = Zero in let b = succ(a) in b → succ(Zero)
    let env = EvalEnv::empty();
    let term = Term::let_in(
        "a",
        Type::Nat,
        Term::Zero,
        Term::let_in("b", Type::Nat, Term::succ(Term::var("a")), Term::var("b")),
    );
    let result = eval_with_env(&term, &env);
    assert_eq!(result, Ok(Term::succ(Term::Zero)));
}

#[test]
fn test_let_with_global_function() {
    // id = λx.x; let y = id(Zero) in y → Zero
    let mut globals = HashMap::new();
    globals.insert(
        "id".to_string(),
        Term::lambda("x", Type::Nat, Term::var("x")),
    );
    let env = EvalEnv::new(globals);
    let term = Term::let_in(
        "y",
        Type::Nat,
        Term::app(Term::Global("id".into()), Term::Zero),
        Term::var("y"),
    );
    let result = eval_with_env(&term, &env);
    assert_eq!(result, Ok(Term::Zero));
}
