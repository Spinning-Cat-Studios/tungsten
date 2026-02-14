//! Term constructors
//!
//! Convenient methods for constructing terms.

use crate::types::Type;

use super::Term;

impl Term {
    // === Constructors ===

    /// Create a variable reference
    pub fn var(name: impl Into<String>) -> Term {
        Term::Var(name.into())
    }

    /// Create a global reference (top-level definition, resolved at runtime)
    pub fn global(name: impl Into<String>) -> Term {
        Term::Global(name.into())
    }

    /// Create a lambda abstraction λx:τ. t
    pub fn lambda(var: impl Into<String>, ty: Type, body: Term) -> Term {
        Term::Lambda(var.into(), ty, Box::new(body))
    }

    /// Create an application t₁ t₂
    #[must_use]
    pub fn app(t1: Term, t2: Term) -> Term {
        Term::App(Box::new(t1), Box::new(t2))
    }

    /// Create a let binding
    pub fn let_in(var: impl Into<String>, ty: Type, def: Term, body: Term) -> Term {
        Term::Let(var.into(), ty, Box::new(def), Box::new(body))
    }

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

    /// Create type abstraction Λα. t
    pub fn ty_abs(var: impl Into<String>, body: Term) -> Term {
        Term::TyAbs(var.into(), Box::new(body))
    }

    /// Create type application t [τ]
    #[must_use]
    pub fn ty_app(t: Term, ty: Type) -> Term {
        Term::TyApp(Box::new(t), ty)
    }

    /// Create reflexivity proof refl [τ] t
    #[must_use]
    pub fn refl(ty: Type, t: Term) -> Term {
        Term::Refl(ty, Box::new(t))
    }

    /// Create substitution subst [τ] [P] `t_eq` `t_proof`
    #[must_use]
    pub fn subst(ty: Type, motive: Type, eq_proof: Term, proof: Term) -> Term {
        Term::Subst(ty, motive, Box::new(eq_proof), Box::new(proof))
    }

    /// Create type annotation (t : τ)
    #[must_use]
    pub fn annot(t: Term, ty: Type) -> Term {
        Term::Annot(Box::new(t), ty)
    }

    // === Phase 2A Constructors ===

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

    /// Create a natural number literal (unary encoding: Succ^n(Zero))
    ///
    /// Warning: This builds n nested Succ terms. For large n, use `nat_lit` instead.
    #[must_use]
    pub fn nat(n: u64) -> Term {
        let mut term = Term::Zero;
        for _ in 0..n {
            term = Term::succ(term);
        }
        term
    }

    /// Create a natural number literal (efficient representation)
    ///
    /// Uses the `NatLit` variant which stores the value directly as u64,
    /// avoiding stack overflow for large numbers.
    #[must_use]
    pub fn nat_lit(n: u64) -> Term {
        Term::NatLit(n)
    }

    /// Create a natural number with automatic representation choice
    ///
    /// Uses unary encoding for small numbers (≤ threshold) for proof compatibility,
    /// and `NatLit` for large numbers to avoid stack overflow.
    #[must_use]
    pub fn nat_smart(n: u64) -> Term {
        const UNARY_THRESHOLD: u64 = 1000;
        if n <= UNARY_THRESHOLD {
            Term::nat(n)
        } else {
            Term::NatLit(n)
        }
    }

    // === Phase 3-Prep Constructors ===

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

    /// Create boolean and: a && b
    #[must_use]
    pub fn bool_and(a: Term, b: Term) -> Term {
        Term::BoolAnd(Box::new(a), Box::new(b))
    }

    /// Create boolean or: a || b
    #[must_use]
    pub fn bool_or(a: Term, b: Term) -> Term {
        Term::BoolOr(Box::new(a), Box::new(b))
    }

    /// Create boolean not: !a
    #[must_use]
    pub fn bool_not(a: Term) -> Term {
        Term::BoolNot(Box::new(a))
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

    /// Create character at index: `char_at` s n
    #[must_use]
    pub fn str_char_at(s: Term, n: Term) -> Term {
        Term::StrCharAt(Box::new(s), Box::new(n))
    }

    /// Create substring: substring s start len
    #[must_use]
    pub fn str_substring(s: Term, start: Term, len: Term) -> Term {
        Term::StrSubstring(Box::new(s), Box::new(start), Box::new(len))
    }

    /// Create extern call: `extern_call` "symbol" [args]
    pub fn extern_call(symbol: impl Into<String>, args: Vec<Term>) -> Term {
        Term::ExternCall(symbol.into(), args)
    }

    /// Create a new ref cell: ref v
    #[must_use]
    pub fn ref_new(v: Term) -> Term {
        Term::RefNew(Box::new(v))
    }

    /// Read from ref: get r
    #[must_use]
    pub fn ref_get(r: Term) -> Term {
        Term::RefGet(Box::new(r))
    }

    /// Write to ref: set r v
    #[must_use]
    pub fn ref_set(r: Term, v: Term) -> Term {
        Term::RefSet(Box::new(r), Box::new(v))
    }

    // === Phase 2B: Flat ADT Constructors (ADR 2.2.26) ===

    /// Create an ADT constructor
    ///
    /// # Arguments
    /// - `adt_ty`: The full ADT type
    /// - `variant_idx`: Index of the constructor (0-based)
    /// - `payload`: The payload term
    #[must_use]
    pub fn adt_construct(adt_ty: Type, variant_idx: usize, payload: Term) -> Term {
        Term::AdtConstruct(adt_ty, variant_idx, Box::new(payload))
    }

    /// Create an ADT match expression
    ///
    /// # Arguments
    /// - `scrutinee`: Term to match on
    /// - `arms`: Vec of (`variant_idx`, `bound_var`, body)
    #[must_use]
    pub fn adt_match(scrutinee: Term, arms: Vec<(usize, String, Box<Term>)>) -> Term {
        Term::AdtMatch(Box::new(scrutinee), arms)
    }

    /// Helper to build ADT match arms
    ///
    /// # Arguments
    /// - `variant_idx`: Index of the constructor
    /// - `var`: Variable name to bind payload
    /// - `body`: Body term for this arm
    pub fn adt_arm(
        variant_idx: usize,
        var: impl Into<String>,
        body: Term,
    ) -> (usize, String, Box<Term>) {
        (variant_idx, var.into(), Box::new(body))
    }
}
