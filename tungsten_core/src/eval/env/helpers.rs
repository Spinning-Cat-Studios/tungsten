//! Helper functions for binary/unary operations in environment-based evaluation.

use crate::terms::Term;

use super::step_with_env;
use super::EvalEnv;
use crate::eval::helpers::{nat_to_term, term_to_nat};
use crate::eval::StepResult;

/// Helper for binary Nat operations with environment
pub(super) fn step_binary_nat_env<F, C>(
    a: &Term,
    b: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(usize, usize) -> usize,
    C: FnOnce(Term, Term) -> Term,
{
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (term_to_nat(a), term_to_nat(b)) {
        (Some(x), Some(y)) => StepResult::Stepped(nat_to_term(op(x, y))),
        _ => StepResult::Stuck,
    }
}

/// Helper for binary Nat->Bool operations with environment
pub(super) fn step_binary_nat_to_bool_env<F, C>(
    a: &Term,
    b: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(usize, usize) -> bool,
    C: FnOnce(Term, Term) -> Term,
{
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (term_to_nat(a), term_to_nat(b)) {
        (Some(x), Some(y)) => {
            if op(x, y) {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

/// Helper for binary Bool operations with environment
pub(super) fn step_binary_bool_env<F, C>(
    a: &Term,
    b: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(bool, bool) -> bool,
    C: FnOnce(Term, Term) -> Term,
{
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (a, b) {
        (Term::True, Term::True) => StepResult::Stepped(if op(true, true) {
            Term::True
        } else {
            Term::False
        }),
        (Term::True, Term::False) => StepResult::Stepped(if op(true, false) {
            Term::True
        } else {
            Term::False
        }),
        (Term::False, Term::True) => StepResult::Stepped(if op(false, true) {
            Term::True
        } else {
            Term::False
        }),
        (Term::False, Term::False) => StepResult::Stepped(if op(false, false) {
            Term::True
        } else {
            Term::False
        }),
        _ => StepResult::Stuck,
    }
}

/// Helper for unary Bool operations with environment
pub(super) fn step_unary_bool_env<F, C>(
    a: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(bool) -> bool,
    C: FnOnce(Term) -> Term,
{
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match a {
        Term::True => StepResult::Stepped(if op(true) { Term::True } else { Term::False }),
        Term::False => StepResult::Stepped(if op(false) { Term::True } else { Term::False }),
        _ => StepResult::Stuck,
    }
}

/// Helper for Nat comparison operations with environment.
///
/// Reduces repetitive NatLt/NatLe/NatGt/NatGe arms to a single helper.
pub(super) fn step_nat_compare_env(
    a: &Term,
    b: &Term,
    cmp: fn(usize, usize) -> bool,
    wrap: fn(Term, Term) -> Term,
    env: &EvalEnv,
) -> StepResult {
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(wrap(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(wrap(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (term_to_nat(a), term_to_nat(b)) {
        (Some(a_val), Some(b_val)) => {
            if cmp(a_val, b_val) {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

// ============================================================================
// General-purpose step pattern helpers (env-aware)
// ============================================================================

/// Evaluate a sub-term to a value with environment. Once it's a value, the whole term is a value.
pub(super) fn step_eval_to_value_env(
    t: &Term,
    wrap: impl FnOnce(Term) -> Term,
    env: &EvalEnv,
) -> StepResult {
    if t.is_value() {
        StepResult::Value
    } else {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => StepResult::Stepped(wrap(t_new)),
            StepResult::Stuck => StepResult::Stuck,
            StepResult::Value => StepResult::Value,
        }
    }
}

/// Evaluate a sub-term with environment; if it's stuck or a value, return Stuck.
pub(super) fn step_eval_then_env(
    t: &Term,
    wrap: impl FnOnce(Term) -> Term,
    env: &EvalEnv,
) -> StepResult {
    if t.is_value() {
        StepResult::Stuck
    } else {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => StepResult::Stepped(wrap(t_new)),
            StepResult::Stuck => StepResult::Stuck,
            StepResult::Value => StepResult::Stuck,
        }
    }
}
