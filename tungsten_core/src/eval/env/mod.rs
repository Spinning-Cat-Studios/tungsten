//! Environment-based evaluation with call-by-need semantics
//!
//! This module provides an evaluator that uses an environment to look up
//! global definitions. Lookups are memoized to provide call-by-need
//! (lazy evaluation with sharing), avoiding exponential term blowup.

mod handlers;
mod handlers_string;
mod helpers;

use std::cell::RefCell;
use std::collections::HashMap;

use crate::terms::Term;

use super::StepResult;

use handlers::{
    step_adt_match_env, step_annot_env, step_app_env, step_case_env, step_fst_env, step_if_env,
    step_let_env, step_natind_env, step_natrec_env, step_pair_env, step_snd_env,
    step_str_char_at_env, step_str_concat_env, step_str_eq_env, step_str_len_env,
    step_str_substring_env, step_subst_env, step_tyapp_env, step_unfold_env,
};
use helpers::{
    step_binary_bool_env, step_binary_nat_env, step_binary_nat_to_bool_env, step_eval_then_env,
    step_eval_to_value_env, step_nat_compare_env, step_unary_bool_env,
};

// ============================================================================
// EvalEnv
// ============================================================================

/// Evaluation environment mapping global names to definitions
///
/// The environment provides call-by-need semantics: when a global is first
/// looked up, its definition is evaluated to a value and cached. Subsequent
/// lookups return the cached value directly.
#[derive(Debug, Clone)]
pub struct EvalEnv {
    /// Map from global names to their unevaluated definitions
    globals: HashMap<String, Term>,
    /// Cache of already-evaluated values (for call-by-need)
    cache: RefCell<HashMap<String, Term>>,
}

impl EvalEnv {
    /// Create a new environment from a map of global definitions
    #[must_use]
    pub fn new(globals: HashMap<String, Term>) -> Self {
        EvalEnv {
            globals,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Create an empty environment
    #[must_use]
    pub fn empty() -> Self {
        EvalEnv::new(HashMap::new())
    }

    /// Look up a global, evaluating and caching if necessary
    pub fn lookup(&self, name: &str) -> Option<Term> {
        if let Some(cached) = self.cache.borrow().get(name) {
            return Some(cached.clone());
        }

        if let Some(def) = self.globals.get(name) {
            let value = eval_with_env(def, self);
            self.cache
                .borrow_mut()
                .insert(name.to_string(), value.clone());
            Some(value)
        } else {
            None
        }
    }
}

// ============================================================================
// Environment-based evaluation
// ============================================================================

/// Evaluate a term to a value using the given environment
pub fn eval_with_env(term: &Term, env: &EvalEnv) -> Term {
    let mut current = term.strip_spans();
    loop {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return current,
        }
    }
}

/// Evaluate with environment and step limit
///
/// Returns `None` if the step limit is exceeded.
pub fn eval_with_env_and_limit(term: &Term, env: &EvalEnv, limit: usize) -> Option<Term> {
    let mut current = term.strip_spans();
    for _ in 0..limit {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return Some(current),
        }
    }
    None
}

// ============================================================================
// step_with_env - Environment-based stepper
// ============================================================================

/// Perform one step of call-by-value evaluation with environment
///
/// This is the environment-aware version of `step()`. Global references
/// are resolved through the environment with call-by-need memoization.
pub fn step_with_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        // Values
        Term::Lambda(_, _, _)
        | Term::TyAbs(_, _)
        | Term::True
        | Term::False
        | Term::Unit
        | Term::Zero
        | Term::NatLit(_)
        | Term::StringLit(_) => StepResult::Value,

        // Stuck terms
        Term::Var(_)
        | Term::Sorry
        | Term::ExternCall(_, _)
        | Term::RefNew(_)
        | Term::RefGet(_)
        | Term::RefSet(_, _) => StepResult::Stuck,

        // Global lookup
        Term::Global(name) => match env.lookup(name) {
            Some(value) => StepResult::Stepped(value),
            None => StepResult::Stuck,
        },

        // Evaluate-to-value wrappers
        Term::Succ(t) => step_eval_to_value_env(t, Term::succ, env),
        Term::Inl(ty, t) => step_eval_to_value_env(t, |t_new| Term::inl(ty.clone(), t_new), env),
        Term::Inr(ty, t) => step_eval_to_value_env(t, |t_new| Term::inr(ty.clone(), t_new), env),
        Term::Refl(ty, t) => step_eval_to_value_env(t, |t_new| Term::refl(ty.clone(), t_new), env),
        Term::Fold(ty, t) => step_eval_to_value_env(t, |t_new| Term::fold(ty.clone(), t_new), env),
        Term::AdtConstruct(adt_ty, idx, payload) => step_eval_to_value_env(
            payload,
            |p_new| Term::adt_construct(adt_ty.clone(), *idx, p_new),
            env,
        ),

        // Evaluate-then-stuck wrapper
        Term::Absurd(ty, t) => step_eval_then_env(t, |t_new| Term::absurd(ty.clone(), t_new), env),

        // Core lambda calculus, products, sums, proof/recursion
        Term::App(..)
        | Term::Let(..)
        | Term::If(..)
        | Term::TyApp(..)
        | Term::Annot(..)
        | Term::Fix(..)
        | Term::Pair(..)
        | Term::Fst(_)
        | Term::Snd(_)
        | Term::Case(..)
        | Term::NatRec(..)
        | Term::NatInd(..)
        | Term::Subst(..)
        | Term::Unfold(..) => step_core_env(term, env),

        // String operations
        Term::StrConcat(..)
        | Term::StrLen(_)
        | Term::StrEq(..)
        | Term::StrCharAt(..)
        | Term::StrSubstring(..) => step_string_env(term, env),

        // Arithmetic, boolean, and nat comparison operations
        Term::NatAdd(..)
        | Term::NatSub(..)
        | Term::NatMul(..)
        | Term::NatDiv(..)
        | Term::NatMod(..)
        | Term::NatEq(..)
        | Term::NatLt(..)
        | Term::NatLe(..)
        | Term::NatGt(..)
        | Term::NatGe(..)
        | Term::BoolAnd(..)
        | Term::BoolOr(..)
        | Term::BoolNot(_) => step_arith_bool_env(term, env),

        // ADT match
        Term::AdtMatch(scrut, arms) => step_adt_match_env(scrut, arms, env),

        // Span wrapper: strip and step inner term
        Term::Spanned(inner, _) => step_with_env(inner, env),

        // Return: evaluate inner, then strip the Return wrapper
        Term::Return(t) => step_return_env(t, env),
    }
}

/// Step a Return term with environment: strip the wrapper when the inner value is ready.
fn step_return_env(t: &Term, env: &EvalEnv) -> StepResult {
    if t.is_value() {
        return StepResult::Stepped(t.clone());
    }
    match step_with_env(t, env) {
        StepResult::Stepped(t_new) => StepResult::Stepped(Term::early_return(t_new)),
        other => other,
    }
}

/// Core term forms with environment: application, let, if, fix, products, sums, proof/recursion.
fn step_core_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::App(t1, t2) => step_app_env(t1, t2, env),
        Term::Let(x, ty, def, body) => step_let_env(x, ty, def, body, env),
        Term::If(cond, then_, else_) => step_if_env(cond, then_, else_, env),
        Term::TyApp(t, ty) => step_tyapp_env(t, ty, env),
        Term::Annot(t, ty) => step_annot_env(t, ty, env),
        Term::Fix(f, ty, body) => {
            let unfolded =
                body.substitute(f, &Term::fix(f.clone(), ty.clone(), body.as_ref().clone()));
            StepResult::Stepped(unfolded)
        }
        Term::Pair(t1, t2) => step_pair_env(t1, t2, env),
        Term::Fst(t) => step_fst_env(t, env),
        Term::Snd(t) => step_snd_env(t, env),
        Term::Case(scrut, x, left, y, right) => {
            use handlers::CaseArm;
            step_case_env(
                scrut,
                &CaseArm { var: x, body: left },
                &CaseArm {
                    var: y,
                    body: right,
                },
                env,
            )
        }
        Term::NatRec(ty, z, s, n) => step_natrec_env(ty, z, s, n, env),
        Term::NatInd(m, z, s, n) => step_natind_env(m, z, s, n, env),
        Term::Subst(ty, motive, eq, proof) => step_subst_env(ty, motive, eq, proof, env),
        Term::Unfold(ty, t) => step_unfold_env(t, ty, env),
        _ => unreachable!("step_core_env called with non-core term"),
    }
}

/// String operation steps with environment.
fn step_string_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::StrConcat(t1, t2) => step_str_concat_env(t1, t2, env),
        Term::StrLen(t) => step_str_len_env(t, env),
        Term::StrEq(t1, t2) => step_str_eq_env(t1, t2, env),
        Term::StrCharAt(s, n) => step_str_char_at_env(s, n, env),
        Term::StrSubstring(s, start, len) => step_str_substring_env(s, start, len, env),
        _ => unreachable!("step_string_env called with non-string term"),
    }
}

/// Arithmetic and boolean operation steps with environment.
fn step_arith_bool_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::NatLt(a, b) => step_nat_compare_env(a, b, |x, y| x < y, Term::nat_lt, env),
        Term::NatLe(a, b) => step_nat_compare_env(a, b, |x, y| x <= y, Term::nat_le, env),
        Term::NatGt(a, b) => step_nat_compare_env(a, b, |x, y| x > y, Term::nat_gt, env),
        Term::NatGe(a, b) => step_nat_compare_env(a, b, |x, y| x >= y, Term::nat_ge, env),
        Term::NatAdd(a, b) => step_binary_nat_env(a, b, usize::saturating_add, Term::nat_add, env),
        Term::NatSub(a, b) => step_binary_nat_env(a, b, usize::saturating_sub, Term::nat_sub, env),
        Term::NatMul(a, b) => step_binary_nat_env(a, b, usize::saturating_mul, Term::nat_mul, env),
        Term::NatDiv(a, b) => step_binary_nat_env(
            a,
            b,
            |x, y| if y == 0 { 0 } else { x / y },
            Term::nat_div,
            env,
        ),
        Term::NatMod(a, b) => step_binary_nat_env(
            a,
            b,
            |x, y| if y == 0 { 0 } else { x % y },
            Term::nat_mod,
            env,
        ),
        Term::NatEq(a, b) => step_binary_nat_to_bool_env(a, b, |x, y| x == y, Term::nat_eq, env),
        Term::BoolAnd(a, b) => step_binary_bool_env(a, b, |x, y| x && y, Term::bool_and, env),
        Term::BoolOr(a, b) => step_binary_bool_env(a, b, |x, y| x || y, Term::bool_or, env),
        Term::BoolNot(a) => step_unary_bool_env(a, |x| !x, Term::bool_not, env),
        _ => unreachable!("step_arith_bool_env called with non-arith/bool term"),
    }
}

// Tests
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
