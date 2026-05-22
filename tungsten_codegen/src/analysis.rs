//! Free Variable Analysis
//!
//! Computes the free variables of a term, which is needed for closure conversion.
//! A variable is free if it's referenced but not bound within the term.

use std::collections::HashSet;
use tungsten_core::terms::{Term, Var};

/// Compute the set of free variables in a term.
pub fn free_vars(term: &Term) -> HashSet<Var> {
    match term {
        Term::Var(x) => {
            let mut set = HashSet::new();
            set.insert(x.clone());
            set
        }

        // Binding forms: remove bound variable from body's free vars
        Term::Lambda(x, _, body) | Term::Fix(x, _, body) => {
            let mut fv = free_vars(body);
            fv.remove(x);
            fv
        }

        Term::Let(x, _, def, body) => {
            let mut fv = free_vars(def);
            let mut body_fv = free_vars(body);
            body_fv.remove(x);
            fv.extend(body_fv);
            fv
        }

        Term::Case(scrut, x, left, y, right) => {
            let mut fv = free_vars(scrut);
            let mut left_fv = free_vars(left);
            left_fv.remove(x);
            let mut right_fv = free_vars(right);
            right_fv.remove(y);
            fv.extend(left_fv);
            fv.extend(right_fv);
            fv
        }

        Term::AdtMatch(scrut, arms) => {
            let mut fv = free_vars(scrut);
            for (_, var, body) in arms {
                let mut arm_fv = free_vars(body);
                arm_fv.remove(var);
                fv.extend(arm_fv);
            }
            fv
        }

        // All other terms: merge free vars from children
        _ => {
            let mut fv = HashSet::new();
            term.for_each_subterm(|child| fv.extend(free_vars(child)));
            fv
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::types::Type;

    #[test]
    fn test_free_var() {
        let term = Term::var("x");
        let fv = free_vars(&term);
        assert!(fv.contains("x"));
        assert_eq!(fv.len(), 1);
    }

    #[test]
    fn test_lambda_binds_var() {
        // λx:Bool. x  has no free variables
        let term = Term::lambda("x", Type::Bool, Term::var("x"));
        let fv = free_vars(&term);
        assert!(fv.is_empty());
    }

    #[test]
    fn test_lambda_free_var() {
        // λx:Bool. y  has y free
        let term = Term::lambda("x", Type::Bool, Term::var("y"));
        let fv = free_vars(&term);
        assert!(fv.contains("y"));
        assert!(!fv.contains("x"));
    }

    #[test]
    fn test_app_free_vars() {
        // f x  has both f and x free
        let term = Term::app(Term::var("f"), Term::var("x"));
        let fv = free_vars(&term);
        assert!(fv.contains("f"));
        assert!(fv.contains("x"));
    }

    #[test]
    fn test_let_binding() {
        // let x = y in x  has y free (x is bound in body)
        let term = Term::let_in("x", Type::Nat, Term::var("y"), Term::var("x"));
        let fv = free_vars(&term);
        assert!(fv.contains("y"));
        assert!(!fv.contains("x"));
    }
}
