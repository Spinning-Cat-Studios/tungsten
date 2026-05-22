//! Standard small-step evaluator (without environment)
//!
//! This module provides the standard small-step call-by-value evaluator
//! that works on closed terms. Global references are stuck - for programs
//! with global definitions, use the environment-based evaluator instead.

mod handlers;

use crate::terms::Term;

use super::helpers::{step_binary_bool, step_binary_nat, step_binary_nat_compare};
use super::StepResult;

use handlers::{
    step_adt_match, step_annot, step_app, step_bool_not, step_case, step_fst, step_if, step_let,
    step_natind, step_natrec, step_pair, step_snd, step_str_char_at, step_str_concat, step_str_eq,
    step_str_len, step_str_substring, step_subst, step_tyapp, step_unfold,
};

// ============================================================================
// step - Standard small-step evaluator
// ============================================================================

/// Perform one step of call-by-value evaluation
///
/// This is the standard small-step evaluator that works on closed terms.
/// Global references are stuck; use [`super::env::step_with_env`] for
/// environment-based evaluation.
#[must_use]
pub fn step(term: &Term) -> StepResult {
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

        // Stuck terms (open/unresolvable)
        Term::Var(_)
        | Term::Global(_)
        | Term::Sorry
        | Term::ExternCall(_, _)
        | Term::RefNew(_)
        | Term::RefGet(_)
        | Term::RefSet(_, _) => StepResult::Stuck,

        // Evaluate-to-value wrappers
        Term::Succ(t) => step_eval_to_value(t, Term::succ),
        Term::Inl(ty, t) => step_eval_to_value(t, |t_new| Term::inl(ty.clone(), t_new)),
        Term::Inr(ty, t) => step_eval_to_value(t, |t_new| Term::inr(ty.clone(), t_new)),
        Term::Refl(ty, t) => step_eval_to_value(t, |t_new| Term::refl(ty.clone(), t_new)),
        Term::Fold(ty, t) => step_eval_to_value(t, |t_new| Term::fold(ty.clone(), t_new)),
        Term::AdtConstruct(adt_ty, idx, payload) => step_eval_to_value(payload, |p_new| {
            Term::adt_construct(adt_ty.clone(), *idx, p_new)
        }),

        // Evaluate-then-stuck wrapper
        Term::Absurd(ty, t) => step_eval_then(t, |t_new| Term::absurd(ty.clone(), t_new)),

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
        | Term::Unfold(..) => step_core(term),

        // String operations
        Term::StrConcat(..)
        | Term::StrLen(_)
        | Term::StrEq(..)
        | Term::StrCharAt(..)
        | Term::StrSubstring(..) => step_string(term),

        // Arithmetic and boolean operations
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
        | Term::BoolNot(_) => step_arith_bool(term),

        // ADT match
        Term::AdtMatch(scrut, arms) => step_adt_match(scrut, arms),

        // Span wrapper: strip and step inner term
        Term::Spanned(inner, _) => step(inner),

        // Return: evaluate inner, then strip the Return wrapper
        Term::Return(t) => step_return(t),
    }
}

/// Step a Return term: strip the wrapper when the inner value is ready.
fn step_return(t: &Term) -> StepResult {
    if t.is_value() {
        return StepResult::Stepped(t.clone());
    }
    match step(t) {
        StepResult::Stepped(t_new) => StepResult::Stepped(Term::early_return(t_new)),
        other => other,
    }
}

// ============================================================================
// Category sub-dispatchers
// ============================================================================

/// Core term forms: application, let, if, fix, products, sums, proof/recursion.
fn step_core(term: &Term) -> StepResult {
    match term {
        Term::App(t1, t2) => step_app(t1, t2),
        Term::Let(x, _, def, body) => step_let(term, x, def, body),
        Term::If(cond, then_, else_) => step_if(cond, then_, else_),
        Term::TyApp(t, ty) => step_tyapp(t, ty),
        Term::Annot(t, _) => step_annot(t),
        Term::Fix(f, ty, body) => {
            let fix_term = Term::Fix(f.clone(), ty.clone(), body.clone());
            StepResult::Stepped(body.substitute(f, &fix_term))
        }
        Term::Pair(t1, t2) => step_pair(t1, t2),
        Term::Fst(t) => step_fst(t),
        Term::Snd(t) => step_snd(t),
        Term::Case(scrut, x, left, y, right) => step_case(scrut, x, left, y, right),
        Term::NatRec(ty, z, s, n) => step_natrec(ty, z, s, n),
        Term::NatInd(m, z, s, n) => step_natind(m, z, s, n),
        Term::Subst(ty, motive, eq, proof) => step_subst(ty, motive, eq, proof),
        Term::Unfold(ty, t) => step_unfold(t, ty),
        _ => unreachable!("step_core called with non-core term"),
    }
}

/// String operation steps.
fn step_string(term: &Term) -> StepResult {
    match term {
        Term::StrConcat(t1, t2) => step_str_concat(t1, t2),
        Term::StrLen(t) => step_str_len(t),
        Term::StrEq(t1, t2) => step_str_eq(t1, t2),
        Term::StrCharAt(s, idx) => step_str_char_at(s, idx),
        Term::StrSubstring(s, start, len) => step_str_substring(s, start, len),
        _ => unreachable!("step_string called with non-string term"),
    }
}

/// Arithmetic and boolean operation steps.
fn step_arith_bool(term: &Term) -> StepResult {
    match term {
        Term::NatAdd(t1, t2) => step_binary_nat(t1, t2, usize::saturating_add),
        Term::NatSub(t1, t2) => step_binary_nat(t1, t2, usize::saturating_sub),
        Term::NatMul(t1, t2) => step_binary_nat(t1, t2, usize::saturating_mul),
        Term::NatDiv(t1, t2) => step_binary_nat(t1, t2, |a, b| if b == 0 { 0 } else { a / b }),
        Term::NatMod(t1, t2) => step_binary_nat(t1, t2, |a, b| if b == 0 { 0 } else { a % b }),
        Term::NatEq(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a == b),
        Term::NatLt(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a < b),
        Term::NatLe(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a <= b),
        Term::NatGt(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a > b),
        Term::NatGe(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a >= b),
        Term::BoolAnd(t1, t2) => step_binary_bool(t1, t2, |a, b| a && b),
        Term::BoolOr(t1, t2) => step_binary_bool(t1, t2, |a, b| a || b),
        Term::BoolNot(t) => step_bool_not(t),
        _ => unreachable!("step_arith_bool called with non-arith/bool term"),
    }
}

// ============================================================================
// Small shared helpers for common step patterns
// ============================================================================

/// Evaluate a sub-term; if it's stuck, return Stuck; if it's a value, return Stuck
/// (no further projection possible from a non-matching value).
fn step_eval_then(t: &Term, wrap: impl FnOnce(Term) -> Term) -> StepResult {
    if t.is_value() {
        StepResult::Stuck
    } else {
        match step(t) {
            StepResult::Stepped(t_new) => StepResult::Stepped(wrap(t_new)),
            StepResult::Stuck => StepResult::Stuck,
            StepResult::Value => StepResult::Stuck,
        }
    }
}

/// Evaluate a sub-term to a value. Once it's a value, the whole term is a value.
fn step_eval_to_value(t: &Term, wrap: impl FnOnce(Term) -> Term) -> StepResult {
    if t.is_value() {
        StepResult::Value
    } else {
        match step(t) {
            StepResult::Stepped(t_new) => StepResult::Stepped(wrap(t_new)),
            StepResult::Stuck => StepResult::Stuck,
            StepResult::Value => StepResult::Value,
        }
    }
}

// Tests
#[cfg(test)]
mod tests;
