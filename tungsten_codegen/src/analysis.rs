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

        Term::Lambda(x, _, body) => {
            let mut fv = free_vars(body);
            fv.remove(x);
            fv
        }

        Term::App(t1, t2) => {
            let mut fv = free_vars(t1);
            fv.extend(free_vars(t2));
            fv
        }

        Term::Let(x, _, def, body) => {
            let mut fv = free_vars(def);
            let mut body_fv = free_vars(body);
            body_fv.remove(x);
            fv.extend(body_fv);
            fv
        }

        Term::True | Term::False | Term::Unit | Term::Zero | Term::Sorry => HashSet::new(),

        Term::StringLit(_) => HashSet::new(),

        Term::If(c, t, e) => {
            let mut fv = free_vars(c);
            fv.extend(free_vars(t));
            fv.extend(free_vars(e));
            fv
        }

        Term::Absurd(_, t) => free_vars(t),

        Term::Succ(t) => free_vars(t),

        Term::NatRec(_, z, s, n) | Term::NatInd(_, z, s, n) => {
            let mut fv = free_vars(z);
            fv.extend(free_vars(s));
            fv.extend(free_vars(n));
            fv
        }

        Term::Pair(t1, t2)
        | Term::StrConcat(t1, t2)
        | Term::StrEq(t1, t2)
        | Term::StrCharAt(t1, t2) => {
            let mut fv = free_vars(t1);
            fv.extend(free_vars(t2));
            fv
        }

        Term::StrSubstring(s, start, len) => {
            let mut fv = free_vars(s);
            fv.extend(free_vars(start));
            fv.extend(free_vars(len));
            fv
        }

        Term::Fst(t) | Term::Snd(t) | Term::StrLen(t) => free_vars(t),

        Term::Inl(_, t) | Term::Inr(_, t) => free_vars(t),

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

        Term::TyAbs(_, body) => free_vars(body),

        Term::TyApp(t, _) => free_vars(t),

        Term::Refl(_, t) => free_vars(t),

        Term::Subst(_, _, eq, proof) => {
            let mut fv = free_vars(eq);
            fv.extend(free_vars(proof));
            fv
        }

        Term::Fix(f, _, body) => {
            let mut fv = free_vars(body);
            fv.remove(f);
            fv
        }

        Term::Fold(_, t) | Term::Unfold(_, t) => free_vars(t),

        Term::Annot(t, _) => free_vars(t),

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 3-Prep: Globals, Nat literals, comparisons, refs, externs
        // ═══════════════════════════════════════════════════════════════════════

        // Global references are resolved at link time, not captured in closures
        Term::Global(_) => HashSet::new(),

        // Natural literals have no free variables
        Term::NatLit(_) => HashSet::new(),

        // Nat arithmetic and comparison operators: collect free vars from both operands
        Term::NatAdd(t1, t2)
        | Term::NatSub(t1, t2)
        | Term::NatMul(t1, t2)
        | Term::NatDiv(t1, t2)
        | Term::NatMod(t1, t2)
        | Term::NatEq(t1, t2)
        | Term::NatLt(t1, t2)
        | Term::NatLe(t1, t2)
        | Term::NatGt(t1, t2)
        | Term::NatGe(t1, t2) => {
            let mut fv = free_vars(t1);
            fv.extend(free_vars(t2));
            fv
        }

        // Boolean operators: collect free vars from both operands
        Term::BoolAnd(t1, t2) | Term::BoolOr(t1, t2) => {
            let mut fv = free_vars(t1);
            fv.extend(free_vars(t2));
            fv
        }

        Term::BoolNot(t) => free_vars(t),

        // External function calls: collect free vars from all arguments
        Term::ExternCall(_, args) => {
            let mut fv = HashSet::new();
            for arg in args {
                fv.extend(free_vars(arg));
            }
            fv
        }

        // Reference operations
        Term::RefNew(t) | Term::RefGet(t) => free_vars(t),

        Term::RefSet(r, v) => {
            let mut fv = free_vars(r);
            fv.extend(free_vars(v));
            fv
        }

        // ═══════════════════════════════════════════════════════════════════════
        // Phase 2B: Flat ADT (ADR 2.2.26)
        // ═══════════════════════════════════════════════════════════════════════
        Term::AdtConstruct(_, _, payload) => free_vars(payload),

        Term::AdtMatch(scrut, arms) => {
            let mut fv = free_vars(scrut);
            for (_, var, body) in arms {
                let mut arm_fv = free_vars(body);
                arm_fv.remove(var); // var is bound in this arm
                fv.extend(arm_fv);
            }
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
