//! Arithmetic, comparison, string, and recursive type term constructors.
//!
//! Nat operations (+, -, *, /, %), comparison operators (<, <=, >, >=, ==),
//! signed `Int` operations (ADR 14.9.26c), string operations (lit, concat,
//! len, eq), and recursive types (fix, fold, unfold).

use serde::{Deserialize, Serialize};

use crate::types::Type;

use super::Term;

/// The operator of an [`Term::IntBin`] node (ADR 14.9.26c).
///
/// One enum rather than ten variants so every `Term` dispatcher gains one arm
/// and the per-operator `match` lives in a function of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl IntBinOp {
    /// The source-level operator, as the printer and the trap message spell it.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            IntBinOp::Add => "+",
            IntBinOp::Sub => "-",
            IntBinOp::Mul => "*",
            IntBinOp::Div => "/",
            IntBinOp::Mod => "%",
            IntBinOp::Eq => "==",
            IntBinOp::Lt => "<",
            IntBinOp::Le => "<=",
            IntBinOp::Gt => ">",
            IntBinOp::Ge => ">=",
        }
    }

    /// `true` for the six comparison operators, whose result is `Bool`.
    #[must_use]
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            IntBinOp::Eq | IntBinOp::Lt | IntBinOp::Le | IntBinOp::Gt | IntBinOp::Ge
        )
    }

    /// The operator's numeric code, as it crosses the FFI (`tg_term_int_bin`).
    #[must_use]
    pub fn code(self) -> u64 {
        self as u64
    }

    /// Inverse of [`IntBinOp::code`]; `None` for a code no operator owns.
    #[must_use]
    pub fn from_code(code: u64) -> Option<IntBinOp> {
        const ALL: [IntBinOp; 10] = [
            IntBinOp::Add,
            IntBinOp::Sub,
            IntBinOp::Mul,
            IntBinOp::Div,
            IntBinOp::Mod,
            IntBinOp::Eq,
            IntBinOp::Lt,
            IntBinOp::Le,
            IntBinOp::Gt,
            IntBinOp::Ge,
        ];
        ALL.get(usize::try_from(code).ok()?).copied()
    }
}

impl Term {
    // === Signed Integer Operations (ADR 14.9.26c) ===

    /// Create a signed integer literal
    #[must_use]
    pub fn int_lit(value: i64) -> Term {
        Term::IntLit(value)
    }

    /// Create a signed binary operation: a `op` b
    #[must_use]
    pub fn int_bin(op: IntBinOp, a: Term, b: Term) -> Term {
        Term::IntBin(op, Box::new(a), Box::new(b))
    }

    /// Create signed negation: −a
    #[must_use]
    pub fn int_neg(a: Term) -> Term {
        Term::IntNeg(Box::new(a))
    }

    /// Create the `Nat → Int` bridge: `to_int(n)`
    #[must_use]
    pub fn nat_to_int(n: Term) -> Term {
        Term::NatToInt(Box::new(n))
    }

    /// Create the `Int → Nat` bridge: `from_int(i)`
    #[must_use]
    pub fn int_to_nat(i: Term) -> Term {
        Term::IntToNat(Box::new(i))
    }

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
