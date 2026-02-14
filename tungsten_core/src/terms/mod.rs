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
mod substitution;

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::types::{TyVar, Type};

/// A term variable name (e.g., x, y, f)
pub type Var = String;

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
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Var(v) => write!(f, "{v}"),
            Term::Global(name) => write!(f, "global:{name}"),
            Term::Lambda(x, ty, body) => write!(f, "(λ{x}:{ty}. {body})"),
            Term::App(t1, t2) => write!(f, "({t1} {t2})"),
            Term::Let(x, ty, def, body) => write!(f, "(let {x} : {ty} = {def} in {body})"),
            Term::True => write!(f, "true"),
            Term::False => write!(f, "false"),
            Term::If(c, t, e) => write!(f, "(if {c} then {t} else {e})"),
            Term::Unit => write!(f, "()"),
            Term::Absurd(ty, t) => write!(f, "(absurd [{ty}] {t})"),
            Term::Zero => write!(f, "zero"),
            Term::Succ(t) => write!(f, "(succ {t})"),
            Term::NatLit(n) => write!(f, "{n}"),
            Term::NatRec(ty, z, s, n) => write!(f, "(natrec [{ty}] {z} {s} {n})"),
            Term::NatInd(p, z, s, n) => write!(f, "(natind [{p}] {z} {s} {n})"),
            // Phase 2A
            Term::StringLit(s) => write!(f, "\"{s}\""),
            Term::StrConcat(t1, t2) => write!(f, "(strconcat {t1} {t2})"),
            Term::StrLen(t) => write!(f, "(strlen {t})"),
            Term::StrEq(t1, t2) => write!(f, "(streq {t1} {t2})"),
            Term::Pair(t1, t2) => write!(f, "({t1}, {t2})"),
            Term::Fst(t) => write!(f, "(fst {t})"),
            Term::Snd(t) => write!(f, "(snd {t})"),
            Term::Inl(ty, t) => write!(f, "(inl [{ty}] {t})"),
            Term::Inr(ty, t) => write!(f, "(inr [{ty}] {t})"),
            Term::Case(scrut, x, t1, y, t2) => {
                write!(f, "(case {scrut} of inl {x} => {t1} | inr {y} => {t2})")
            }
            Term::TyAbs(alpha, body) => write!(f, "(Λ{alpha}. {body})"),
            Term::TyApp(t, ty) => write!(f, "({t} [{ty}])"),
            Term::Refl(ty, t) => write!(f, "(refl [{ty}] {t})"),
            Term::Subst(ty, p, eq, proof) => write!(f, "(subst [{ty}] [{p}] {eq} {proof})"),
            // Phase 2A
            Term::Fix(f_var, ty, body) => write!(f, "(fix {f_var}:{ty}. {body})"),
            Term::Fold(ty, t) => write!(f, "(fold [{ty}] {t})"),
            Term::Unfold(ty, t) => write!(f, "(unfold [{ty}] {t})"),
            Term::Annot(t, ty) => write!(f, "({t} : {ty})"),
            Term::Sorry => write!(f, "sorry"),
            // Phase 3C arithmetic
            Term::NatAdd(t1, t2) => write!(f, "({t1} + {t2})"),
            Term::NatSub(t1, t2) => write!(f, "({t1} - {t2})"),
            Term::NatMul(t1, t2) => write!(f, "({t1} * {t2})"),
            Term::NatDiv(t1, t2) => write!(f, "({t1} / {t2})"),
            Term::NatMod(t1, t2) => write!(f, "({t1} % {t2})"),
            Term::NatEq(t1, t2) => write!(f, "({t1} == {t2})"),
            // Phase 3-Prep comparisons
            Term::NatLt(t1, t2) => write!(f, "({t1} < {t2})"),
            Term::NatLe(t1, t2) => write!(f, "({t1} <= {t2})"),
            Term::NatGt(t1, t2) => write!(f, "({t1} > {t2})"),
            Term::NatGe(t1, t2) => write!(f, "({t1} >= {t2})"),
            // Phase 3C boolean
            Term::BoolAnd(t1, t2) => write!(f, "({t1} && {t2})"),
            Term::BoolOr(t1, t2) => write!(f, "({t1} || {t2})"),
            Term::BoolNot(t) => write!(f, "(!{t})"),
            // Phase 3-Prep strings
            Term::StrCharAt(s, idx) => write!(f, "(char_at {s} {idx})"),
            Term::StrSubstring(s, start, len) => write!(f, "(substring {s} {start} {len})"),
            Term::ExternCall(name, args) => {
                write!(f, "(extern_call {name} ")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            Term::RefNew(t) => write!(f, "(ref {t})"),
            Term::RefGet(t) => write!(f, "(ref_get {t})"),
            Term::RefSet(r, v) => write!(f, "(ref_set {r} {v})"),
            // Phase 2B: Flat ADT
            Term::AdtConstruct(adt_ty, idx, payload) => {
                write!(f, "(adt_construct [{adt_ty}] {idx} {payload})")
            }
            Term::AdtMatch(scrut, arms) => {
                write!(f, "(adt_match {scrut} [")?;
                for (i, (idx, var, body)) in arms.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{idx} {var} => {body}")?;
                }
                write!(f, "])")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_literal() {
        assert_eq!(Term::nat(0), Term::Zero);
        assert_eq!(Term::nat(1), Term::succ(Term::Zero));
        assert_eq!(Term::nat(2), Term::succ(Term::succ(Term::Zero)));
    }

    #[test]
    fn test_is_value() {
        assert!(Term::lambda("x", Type::Nat, Term::var("x")).is_value());
        assert!(Term::True.is_value());
        assert!(Term::False.is_value());
        assert!(Term::Zero.is_value());
        assert!(Term::succ(Term::Zero).is_value());
        assert!(Term::Unit.is_value());
        assert!(Term::pair(Term::Zero, Term::True).is_value());
        assert!(!Term::app(Term::var("f"), Term::var("x")).is_value());
        assert!(!Term::fst(Term::pair(Term::Zero, Term::True)).is_value());
    }

    #[test]
    fn test_term_substitution() {
        // (λx. x) → (λx. x) (no change, x is bound)
        let id = Term::lambda("x", Type::Nat, Term::var("x"));
        let result = id.substitute("x", &Term::Zero);
        assert_eq!(result, id);

        // y[y := zero] = zero
        let y = Term::var("y");
        let result = y.substitute("y", &Term::Zero);
        assert_eq!(result, Term::Zero);

        // (λx. y)[y := zero] = λx. zero
        let term = Term::lambda("x", Type::Nat, Term::var("y"));
        let result = term.substitute("y", &Term::Zero);
        assert_eq!(result, Term::lambda("x", Type::Nat, Term::Zero));
    }

    #[test]
    fn test_free_vars() {
        let id = Term::lambda("x", Type::Nat, Term::var("x"));
        assert!(id.free_vars().is_empty());

        let open = Term::lambda("x", Type::Nat, Term::var("y"));
        assert!(open.free_vars().contains("y"));
        assert!(!open.free_vars().contains("x"));

        let app = Term::app(Term::var("f"), Term::var("x"));
        assert!(app.free_vars().contains("f"));
        assert!(app.free_vars().contains("x"));
    }

    #[test]
    fn test_display() {
        let id = Term::lambda("x", Type::Nat, Term::var("x"));
        assert_eq!(id.to_string(), "(λx:Nat. x)");

        let app = Term::app(Term::var("f"), Term::Zero);
        assert_eq!(app.to_string(), "(f zero)");
    }
}
