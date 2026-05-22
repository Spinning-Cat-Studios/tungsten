//! Arithmetic, comparison, string, and recursive type term constructors.
//!
//! Nat operations (+, -, *, /, %), comparison operators (<, <=, >, >=, ==),
//! string operations (lit, concat, len, eq), and recursive types (fix, fold, unfold).

use crate::types::Type;

use super::Term;

impl Term {
    /// Create natural addition: a + b
    #[must_use]
    pub fn nat_add(a: Term, b: Term) -> Term {
        Term::NatAdd(Box::new(a), Box::new(b))
    }

    /// Create natural subtraction: a - b (saturating at 0)
    #[must_use]
    pub fn nat_sub(a: Term, b: Term) -> Term {
        Term::NatSub(Box::new(a), Box::new(b))
    }

    /// Create natural multiplication: a * b
    #[must_use]
    pub fn nat_mul(a: Term, b: Term) -> Term {
        Term::NatMul(Box::new(a), Box::new(b))
    }

    /// Create natural division: a / b
    #[must_use]
    pub fn nat_div(a: Term, b: Term) -> Term {
        Term::NatDiv(Box::new(a), Box::new(b))
    }

    /// Create natural modulo: a % b
    #[must_use]
    pub fn nat_mod(a: Term, b: Term) -> Term {
        Term::NatMod(Box::new(a), Box::new(b))
    }

    /// Create natural equality: a == b
    #[must_use]
    pub fn nat_eq(a: Term, b: Term) -> Term {
        Term::NatEq(Box::new(a), Box::new(b))
    }

    /// Create natural less than: a < b
    #[must_use]
    pub fn nat_lt(a: Term, b: Term) -> Term {
        Term::NatLt(Box::new(a), Box::new(b))
    }

    /// Create natural less than or equal: a <= b
    #[must_use]
    pub fn nat_le(a: Term, b: Term) -> Term {
        Term::NatLe(Box::new(a), Box::new(b))
    }

    /// Create natural greater than: a > b
    #[must_use]
    pub fn nat_gt(a: Term, b: Term) -> Term {
        Term::NatGt(Box::new(a), Box::new(b))
    }

    /// Create natural greater than or equal: a >= b
    #[must_use]
    pub fn nat_ge(a: Term, b: Term) -> Term {
        Term::NatGe(Box::new(a), Box::new(b))
    }

    // === String Operations ===

    /// Create a string literal
    pub fn string_lit(s: impl Into<String>) -> Term {
        Term::StringLit(s.into())
    }

    /// Create string concatenation
    #[must_use]
    pub fn str_concat(t1: Term, t2: Term) -> Term {
        Term::StrConcat(Box::new(t1), Box::new(t2))
    }

    /// Create string length
    #[must_use]
    pub fn str_len(t: Term) -> Term {
        Term::StrLen(Box::new(t))
    }

    /// Create string equality
    #[must_use]
    pub fn str_eq(t1: Term, t2: Term) -> Term {
        Term::StrEq(Box::new(t1), Box::new(t2))
    }

    // === Recursive Type Operations ===

    /// Create fixed point: fix f:τ. t
    pub fn fix(var: impl Into<String>, ty: Type, body: Term) -> Term {
        Term::Fix(var.into(), ty, Box::new(body))
    }

    /// Create fold: fold [μα.τ] t
    #[must_use]
    pub fn fold(mu_ty: Type, t: Term) -> Term {
        Term::Fold(mu_ty, Box::new(t))
    }

    /// Create unfold: unfold [μα.τ] t
    #[must_use]
    pub fn unfold(mu_ty: Type, t: Term) -> Term {
        Term::Unfold(mu_ty, Box::new(t))
    }
}
