//! Data type constructors for Term.
//!
//! Constructors for natural numbers, products, sums, proofs, and type-level operations.

use crate::types::Type;

use super::Term;

impl Term {
    /// Create an if-then-else
    #[must_use]
    pub fn if_then_else(cond: Term, then_: Term, else_: Term) -> Term {
        Term::If(Box::new(cond), Box::new(then_), Box::new(else_))
    }

    /// Create an absurd elimination
    #[must_use]
    pub fn absurd(ty: Type, t: Term) -> Term {
        Term::Absurd(ty, Box::new(t))
    }

    /// Create a successor
    #[must_use]
    pub fn succ(t: Term) -> Term {
        Term::Succ(Box::new(t))
    }

    /// Create natrec
    #[must_use]
    pub fn natrec(ty: Type, zero_case: Term, succ_case: Term, n: Term) -> Term {
        Term::NatRec(ty, Box::new(zero_case), Box::new(succ_case), Box::new(n))
    }

    /// Create natind
    #[must_use]
    pub fn natind(motive: Type, zero_case: Term, succ_case: Term, n: Term) -> Term {
        Term::NatInd(
            motive,
            Box::new(zero_case),
            Box::new(succ_case),
            Box::new(n),
        )
    }

    /// Create a pair
    #[must_use]
    pub fn pair(t1: Term, t2: Term) -> Term {
        Term::Pair(Box::new(t1), Box::new(t2))
    }

    /// Create fst projection
    #[must_use]
    pub fn fst(t: Term) -> Term {
        Term::Fst(Box::new(t))
    }

    /// Create snd projection
    #[must_use]
    pub fn snd(t: Term) -> Term {
        Term::Snd(Box::new(t))
    }

    /// Create left injection inl [τ] t
    #[must_use]
    pub fn inl(sum_ty: Type, t: Term) -> Term {
        Term::Inl(sum_ty, Box::new(t))
    }

    /// Create right injection inr [τ] t
    #[must_use]
    pub fn inr(sum_ty: Type, t: Term) -> Term {
        Term::Inr(sum_ty, Box::new(t))
    }

    /// Create case analysis
    pub fn case(
        scrutinee: Term,
        left_var: impl Into<String>,
        left_body: Term,
        right_var: impl Into<String>,
        right_body: Term,
    ) -> Term {
        Term::Case(
            Box::new(scrutinee),
            left_var.into(),
            Box::new(left_body),
            right_var.into(),
            Box::new(right_body),
        )
    }
}
