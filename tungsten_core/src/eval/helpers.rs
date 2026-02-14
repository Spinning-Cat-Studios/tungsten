//! Helper functions for evaluation
//!
//! This module provides utility functions used by both the standard
//! and environment-based evaluators.

use crate::terms::Term;

use super::StepResult;

// ============================================================================
// Nat conversion helpers
// ============================================================================

/// Convert a Nat term (zero/succ chain or `NatLit`) to a usize
///
/// Returns `None` if the term is not a normalized natural number.
#[must_use]
pub fn term_to_nat(term: &Term) -> Option<usize> {
    match term {
        Term::Zero => Some(0),
        Term::Succ(inner) => term_to_nat(inner).map(|n| n + 1),
        Term::NatLit(n) => Some(*n as usize),
        _ => None,
    }
}

/// Convert a usize to a Nat term (zero/succ chain)
#[must_use]
pub fn nat_to_term(n: usize) -> Term {
    let mut term = Term::Zero;
    for _ in 0..n {
        term = Term::succ(term);
    }
    term
}

// ============================================================================
// Binary comparison helper
// ============================================================================

/// Helper for binary Nat comparison operations
///
/// Evaluates both operands and applies the comparison function.
/// Note: This function uses the environment-free `step()` internally,
/// so it's only suitable for the non-environment evaluator.
pub(crate) fn step_binary_nat_compare<F>(t1: &Term, t2: &Term, cmp: F) -> StepResult
where
    F: Fn(usize, usize) -> bool,
{
    use super::step::step;

    // Evaluate left operand first
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                // Note: We can't preserve the exact operation type here
                // The caller handles the reconstruction
                return StepResult::Stepped(Term::NatLt(Box::new(t1_new), Box::new(t2.clone())));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Evaluate right operand
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::NatLt(Box::new(t1.clone()), Box::new(t2_new)));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both values: compare
    match (term_to_nat(t1), term_to_nat(t2)) {
        (Some(a), Some(b)) => {
            if cmp(a, b) {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

// ============================================================================
// Binary Nat arithmetic helper
// ============================================================================

/// Helper for binary Nat arithmetic operations (add, sub, mul, div, mod)
///
/// Evaluates both operands and applies the arithmetic function.
pub(crate) fn step_binary_nat<F>(t1: &Term, t2: &Term, op: F) -> StepResult
where
    F: Fn(usize, usize) -> usize,
{
    use super::step::step;

    // Evaluate left operand first
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::NatAdd(Box::new(t1_new), Box::new(t2.clone())));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Evaluate right operand
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::NatAdd(Box::new(t1.clone()), Box::new(t2_new)));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both values: compute
    match (term_to_nat(t1), term_to_nat(t2)) {
        (Some(a), Some(b)) => {
            let result = op(a, b);
            StepResult::Stepped(nat_to_term(result))
        }
        _ => StepResult::Stuck,
    }
}

// ============================================================================
// Binary Bool helper
// ============================================================================

/// Helper for binary Bool operations (and, or)
///
/// Evaluates both operands and applies the boolean function.
pub(crate) fn step_binary_bool<F>(t1: &Term, t2: &Term, op: F) -> StepResult
where
    F: Fn(bool, bool) -> bool,
{
    use super::step::step;

    // Helper to convert Term to bool
    fn term_to_bool(t: &Term) -> Option<bool> {
        match t {
            Term::True => Some(true),
            Term::False => Some(false),
            _ => None,
        }
    }

    // Evaluate left operand first
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::BoolAnd(Box::new(t1_new), Box::new(t2.clone())));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Evaluate right operand
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::BoolAnd(Box::new(t1.clone()), Box::new(t2_new)));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both values: compute
    match (term_to_bool(t1), term_to_bool(t2)) {
        (Some(a), Some(b)) => {
            if op(a, b) {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

// ============================================================================
// Term helper methods
// ============================================================================

/// Helper method on Term to get let binding type
impl Term {
    pub(crate) fn let_type(&self) -> Option<&crate::types::Type> {
        match self {
            Term::Let(_, ty, _, _) => Some(ty),
            _ => None,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_term_to_nat_zero() {
        assert_eq!(term_to_nat(&Term::Zero), Some(0));
    }

    #[test]
    fn test_term_to_nat_succ() {
        let three = Term::succ(Term::succ(Term::succ(Term::Zero)));
        assert_eq!(term_to_nat(&three), Some(3));
    }

    #[test]
    fn test_term_to_nat_non_nat() {
        assert_eq!(term_to_nat(&Term::Unit), None);
        assert_eq!(term_to_nat(&Term::True), None);
    }

    #[test]
    fn test_nat_to_term_zero() {
        assert_eq!(nat_to_term(0), Term::Zero);
    }

    #[test]
    fn test_nat_to_term_positive() {
        let result = nat_to_term(3);
        let expected = Term::succ(Term::succ(Term::succ(Term::Zero)));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_roundtrip() {
        for n in 0..10 {
            let term = nat_to_term(n);
            assert_eq!(term_to_nat(&term), Some(n));
        }
    }
}
