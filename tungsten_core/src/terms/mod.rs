//! Phase 1 Core Terms
//!
//! Defines the term syntax for the Tungsten core calculus:
//! t ::= x | λx:τ. t | t t | let x : τ = t in t | true | false | if t then t else t
//!     | () | absurd [τ] t | zero | succ t | natrec [τ] t t t | natind [P] t t t
//!     | (t, t) | fst t | snd t | inl [τ + τ] t | inr [τ + τ] t | case t of inl x => t | inr y => t
//!     | Λα. t | t [τ] | refl [τ] t | subst [τ] [P] t t | (t : τ) | sorry
//!     | "s" | strconcat t t | strlen t | streq t t  (Phase 2A)
//!     | fix f:τ. t | fold [μα.τ] t | unfold [μα.τ] t  (Phase 2A)
//!     | extern "sym" | a < b | a <= b | a > b | a >= b  (Phase 3-Prep)
//!     | `char_at` s n | ref v | get r | set r v  (Phase 3-Prep)

mod analysis;
mod constructors;
mod constructors_arith;
mod constructors_data;
mod constructors_native;
mod display;
mod substitution;
mod traversal;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::types::{TyVar, Type};

/// A term variable name (e.g., x, y, f)
pub type Var = String;

/// A byte-offset source span for debug locations (ADR 17.4.26a §3.1).
///
/// Half-open interval `[start, end)`. Mirrors `bootstrap::Span` but lives in
/// `tungsten_core` so that `SpannedTerm` has no cross-crate dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TermSpan {
    /// Start byte offset (inclusive)
    pub start: u32,
    /// End byte offset (exclusive)
    pub end: u32,
}

impl TermSpan {
    /// Create a new span from start and end byte offsets.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// A term wrapped with an optional source span (ADR 17.4.26a §3.1, Approach B).
///
/// Provides sub-expression debug locations without modifying the `Term` enum.
/// `span` is `None` for compiler-generated terms (desugaring, monomorphization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpannedTerm {
    /// The underlying term (unchanged)
    pub term: Term,
    /// Source span, if this term originated from user source code
    pub span: Option<TermSpan>,
}

impl SpannedTerm {
    /// Wrap a term with a known source span.
    #[must_use]
    pub fn new(term: Term, span: TermSpan) -> Self {
        Self {
            term,
            span: Some(span),
        }
    }

    /// Wrap a compiler-generated term (no source span).
    #[must_use]
    pub fn generated(term: Term) -> Self {
        Self { term, span: None }
    }
}

impl std::ops::Deref for SpannedTerm {
    type Target = Term;
    fn deref(&self) -> &Term {
        &self.term
    }
}

impl fmt::Display for SpannedTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.term.fmt(f)
    }
}

impl PartialEq<Term> for SpannedTerm {
    fn eq(&self, other: &Term) -> bool {
        self.term == *other
    }
}

/// Phase 1 Core Terms
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    // === Variables ===
    /// Variable reference (local: lambda-bound, let-bound, pattern-bound)
    Var(Var),

    /// Reference to a top-level global definition (resolved at runtime via environment)
    Global(String),

    // === Lambda calculus ===
    /// Lambda abstraction λx:τ. t
    Lambda(Var, Type, Box<Term>),

    /// Application t₁ t₂
    App(Box<Term>, Box<Term>),

    /// Let binding: let x : τ = t₁ in t₂
    Let(Var, Type, Box<Term>, Box<Term>),

    // === Booleans ===
    /// Boolean true
    True,

    /// Boolean false
    False,

    /// If-then-else: if `t_cond` then `t_then` else `t_else`
    If(Box<Term>, Box<Term>, Box<Term>),

    // === Unit ===
    /// Unit value ()
    Unit,

    // === Void ===
    /// Absurd elimination: absurd [τ] t (ex falso quodlibet)
    Absurd(Type, Box<Term>),

    // === Naturals ===
    /// Zero
    Zero,

    /// Successor: succ t
    Succ(Box<Term>),

    /// Natural number literal (efficient representation for large numbers)
    ///
    /// This is operationally equivalent to `Succ^n(Zero)` but avoids
    /// stack overflow for large numbers. Small numbers (≤1000) should
    /// use Zero/Succ for proof compatibility with `NatInd`.
    NatLit(u64),

    /// Primitive recursion: natrec [τ] `t_zero` `t_succ` `t_n`
    /// - τ is the result type
    /// - `t_zero` : τ (base case)
    /// - `t_succ` : Nat → τ → τ (recursive case)
    /// - `t_n` : Nat (the number to recurse on)
    NatRec(Type, Box<Term>, Box<Term>, Box<Term>),

    /// Induction: natind [P] `p_zero` `p_succ` `t_n`
    /// - P : Nat → Prop (the motive)
    /// - `p_zero` : P zero
    /// - `p_succ` : ∀n:Nat. P n → P (succ n)
    /// - `t_n` : Nat
    NatInd(Type, Box<Term>, Box<Term>, Box<Term>),

    // === Nat Arithmetic (Phase 3C) ===
    /// Natural addition: a + b
    NatAdd(Box<Term>, Box<Term>),
    /// Natural subtraction: a - b (saturating at 0)
    NatSub(Box<Term>, Box<Term>),
    /// Natural multiplication: a * b
    NatMul(Box<Term>, Box<Term>),
    /// Natural division: a / b
    NatDiv(Box<Term>, Box<Term>),
    /// Natural modulo: a % b
    NatMod(Box<Term>, Box<Term>),
    /// Natural equality: a == b
    NatEq(Box<Term>, Box<Term>),

    // === Integer Comparison (Phase 3-Prep) ===
    /// Natural less than: a < b
    NatLt(Box<Term>, Box<Term>),
    /// Natural less than or equal: a <= b
    NatLe(Box<Term>, Box<Term>),
    /// Natural greater than: a > b
    NatGt(Box<Term>, Box<Term>),
    /// Natural greater than or equal: a >= b
    NatGe(Box<Term>, Box<Term>),

    // === Boolean Operations (Phase 3C) ===
    /// Boolean AND: a && b
    BoolAnd(Box<Term>, Box<Term>),
    /// Boolean OR: a || b
    BoolOr(Box<Term>, Box<Term>),
    /// Boolean NOT: !a
    BoolNot(Box<Term>),

    // === Strings (Phase 2A) ===
    /// String literal: "hello"
    StringLit(String),

    /// String concatenation: strconcat t₁ t₂
    StrConcat(Box<Term>, Box<Term>),

    /// String length: strlen t
    StrLen(Box<Term>),

    /// String equality: streq t₁ t₂
    StrEq(Box<Term>, Box<Term>),

    /// Character at index: `char_at` s n → Nat (ASCII code)
    StrCharAt(Box<Term>, Box<Term>),

    /// Substring: substring s start len → String (Phase 3A)
    StrSubstring(Box<Term>, Box<Term>, Box<Term>),

    // === Products (Pairs) ===
    /// Pair construction (t₁, t₂)
    Pair(Box<Term>, Box<Term>),

    /// First projection: fst t
    Fst(Box<Term>),

    /// Second projection: snd t
    Snd(Box<Term>),

    // === Sums ===
    /// Left injection: inl [τ₁ + τ₂] t
    Inl(Type, Box<Term>),

    /// Right injection: inr [τ₁ + τ₂] t
    Inr(Type, Box<Term>),

    /// Case analysis: case t of inl x => t₁ | inr y => t₂
    Case(Box<Term>, Var, Box<Term>, Var, Box<Term>),

    // === Polymorphism ===
    /// Type abstraction: Λα. t
    TyAbs(TyVar, Box<Term>),

    /// Type application: t [τ]
    TyApp(Box<Term>, Type),

    // === Equality ===
    /// Reflexivity: refl [τ] t  produces Eq τ t t
    Refl(Type, Box<Term>),

    /// Substitution: subst [τ] [P] `t_eq` `t_proof`
    /// If P : τ → Prop, `t_eq` : Eq τ a b, `t_proof` : P a
    /// Then subst produces P b
    Subst(Type, Type, Box<Term>, Box<Term>),

    // === Recursion (Phase 2A) ===
    /// Fixed point: fix f:τ. t
    /// - f : τ is the recursive function being defined
    /// - t : τ is the body (may reference f)
    /// - Computes the least fixed point
    Fix(Var, Type, Box<Term>),

    // === Recursive Types (Phase 2A) ===
    /// Fold: fold [μα.τ] t
    /// - Packs a value into a recursive type
    /// - t : τ[α := μα.τ]
    /// - Result: μα.τ
    Fold(Type, Box<Term>),

    /// Unfold: unfold [μα.τ] t
    /// - Unpacks a recursive type
    /// - t : μα.τ
    /// - Result: τ[α := μα.τ]
    Unfold(Type, Box<Term>),

    // === FFI (Phase 3-Prep) ===
    /// External function call: `extern_call` "symbol" [args]
    ExternCall(String, Vec<Term>),

    // === Ref Cells (Phase 3-Prep) ===
    /// Create a new ref cell: ref v
    RefNew(Box<Term>),
    /// Read from ref: get r
    RefGet(Box<Term>),
    /// Write to ref: set r v
    RefSet(Box<Term>, Box<Term>),

    // === Meta ===
    /// Type annotation: (t : τ)
    Annot(Box<Term>, Type),

    /// Sorry (unsafe axiom) - type-checks as any type
    Sorry,

    // === Flat ADT (Phase 2B - ADR 2.2.26) ===
    /// ADT constructor: constructs variant `idx` of ADT type `adt_ty` with payload `payload`
    ///
    /// For `TokenKind::TokIntLit(42)`:
    /// - `adt_ty`: `Type::Adt("TokenKind`", [], [("`TokIntLit`", Nat), ...])
    /// - idx: 0 (the variant index in declaration order)
    /// - payload: Term representing 42
    AdtConstruct(Type, usize, Box<Term>),

    /// ADT pattern match: switch on tag, dispatch to arms
    ///
    /// - scrutinee: Term to match on (must be ADT type)
    /// - arms: Vec of (`variant_idx`, `bound_var`, body)
    ///   - `variant_idx`: Index of constructor being matched
    ///   - `bound_var`: Variable bound to payload in body
    ///   - body: Term to evaluate when this arm matches
    ///
    /// Generated as LLVM switch instruction for O(1) dispatch.
    AdtMatch(Box<Term>, Vec<(usize, Var, Box<Term>)>),

    // === Control Flow ===
    /// Early return: return e
    /// Type rule: Γ ⊢ e : T, T = declared return type ⟹ Γ ⊢ return e : ⊥
    Return(Box<Term>),

    // === Span Wrapper (ADR 17.4.26a §3.1) ===
    /// Source span wrapper for sub-expression debug locations.
    ///
    /// Transparent to semantics (evaluation, typing, substitution) —
    /// codegen sets `set_current_debug_location()` on encountering this.
    Spanned(Box<Term>, TermSpan),
}

#[cfg(test)]
mod tests;
