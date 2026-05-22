use std::collections::HashMap;

use super::*;
use crate::types::Type;

#[test]
fn test_eval_env_empty() {
    let env = EvalEnv::empty();
    assert!(env.lookup("foo").is_none());
}

#[test]
fn test_eval_env_lookup() {
    let mut globals = HashMap::new();
    globals.insert("x".to_string(), Term::Zero);
    let env = EvalEnv::new(globals);

    assert_eq!(env.lookup("x"), Some(Term::Zero));
    assert!(env.lookup("y").is_none());
}

#[test]
fn test_global_lookup() {
    let mut globals = HashMap::new();
    globals.insert("x".to_string(), Term::Zero);
    let env = EvalEnv::new(globals);

    let result = eval_with_env(&Term::Global("x".into()), &env);
    assert_eq!(result, Term::Zero);
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
    assert_eq!(result1, Some(Term::Zero));

    // Second lookup should return cached value
    let result2 = env.lookup("x");
    assert_eq!(result2, Some(Term::Zero));

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
    assert_eq!(result, Term::Zero);
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
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_eval_with_env_and_limit_terminates() {
    let env = EvalEnv::empty();
    let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
    let result = eval_with_env_and_limit(&term, &env, 100);
    assert_eq!(result, Some(Term::Zero));
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
    assert_eq!(result, Term::succ(Term::Zero));
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
    assert_ne!(result, Term::succ(Term::Zero));
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
    assert_eq!(result, Term::succ(Term::Zero));
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
    assert_eq!(result, Term::Zero);
}
