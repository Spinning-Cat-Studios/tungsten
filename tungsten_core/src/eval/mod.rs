//! # Evaluation Module
//!
//! This module provides a small-step call-by-value evaluator for the
//! Tungsten core calculus.
//!
//! ## Overview
//!
//! The evaluator implements the standard operational semantics for a
//! dependently-typed lambda calculus with:
//!
//! - Lambda abstraction and application (β-reduction)
//! - Let bindings (substitution)
//! - Booleans with if-then-else
//! - Natural numbers with primitive recursion (natrec)
//! - Products (pairs) and sums (tagged unions)
//! - Polymorphism (type abstraction/application)
//! - Propositional equality (refl and subst)
//! - Strings with concatenation, length, and equality
//! - General recursion (fix combinator)
//! - Recursive types (fold/unfold for μ-types)
//! - Nat comparison operations (lt, le, gt, ge)
//! - String character access (`char_at`, substring)
//! - FFI calls (`extern_call`) - stuck in pure evaluation
//! - Mutable references (`ref_new`, `ref_get`, `ref_set`) - stuck in pure evaluation
//!
//! ## Evaluation Strategies
//!
//! The module provides two evaluation strategies:
//!
//! ### Without Environment (`step`, `eval`)
//!
//! The standard small-step evaluator that works on closed terms.
//! Global references are stuck - this is suitable for evaluating
//! self-contained expressions.
//!
//! ### With Environment (`step_with_env`, `eval_with_env`)
//!
//! Environment-based evaluation with call-by-need semantics. The evaluator
//! maintains an `EvalEnv` that maps global names to their definitions.
//! Lookups are memoized to provide call-by-need (lazy evaluation with sharing).
//!
//! This is the recommended approach for evaluating programs with multiple
//! definitions, as it avoids exponential term blowup from naive substitution.

mod env;
mod helpers;
mod step;

// Re-export main types and functions
pub use env::{eval_with_env, eval_with_env_and_limit, EvalEnv};
pub use helpers::{nat_to_term, term_to_nat};
pub use step::step;

use crate::terms::Term;

// ============================================================================
// StepResult
// ============================================================================

/// Result of one step of evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult {
    /// Term reduced to a new term
    Stepped(Term),
    /// Term is already a value (cannot step further)
    Value,
    /// Term is stuck (e.g., open term, sorry)
    Stuck,
}

// ============================================================================
// Public evaluation functions
// ============================================================================

/// Evaluate a term to a value (or stuck state)
///
/// Uses the environment-free evaluator. Global references will be stuck.
/// For programs with global definitions, use [`eval_with_env`] instead.
#[must_use]
pub fn eval(term: &Term) -> Term {
    let mut current = term.clone();
    loop {
        match step(&current) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return current,
        }
    }
}

/// Evaluate with step limit (to detect non-termination)
///
/// Returns `None` if the step limit is exceeded, `Some(value)` otherwise.
/// Uses the environment-free evaluator.
#[must_use]
pub fn eval_with_limit(term: &Term, limit: usize) -> Option<Term> {
    let mut current = term.clone();
    for _ in 0..limit {
        match step(&current) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return Some(current),
        }
    }
    None // Step limit exceeded
}
