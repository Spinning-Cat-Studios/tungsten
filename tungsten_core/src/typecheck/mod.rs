//! Type Checker
//!
//! Implements the typing judgment Γ ⊢ t : τ for the Tungsten core calculus.
//!
//! The type checker synthesizes types from terms (bidirectional type checking).
//!
//! ## Overview
//!
//! This module provides:
//!
//! - [`type_of`]: Synthesize the type of a term given a context
//! - [`check`]: Check a term against an expected type
//! - [`types_equal`]: Check if two types are definitionally equal (α-equivalent)
//! - [`check_type_wf`]: Verify a type is well-formed under a context
//!
//! ## Typing Rules
//!
//! The type checker implements standard typing rules for:
//!
//! - Lambda calculus (λ, application, let)
//! - Booleans and conditionals
//! - Natural numbers with recursion (natrec, natind)
//! - Products (pairs) and sums (tagged unions)
//! - Polymorphism (∀, type application)
//! - Propositional equality (refl, subst)
//! - Strings (Phase 2A)
//! - General recursion via fix (Phase 2A)
//! - Recursive types μα.τ (Phase 2A)
//! - Nat comparisons and string operations (Phase 3-Prep)
//! - Mutable references (Phase 3-Prep)

mod error;
mod rules;

// Re-export public API
pub use error::TypeError;
pub use rules::{check, check_type_wf, type_of, types_equal};

/// Result type for type checking
pub type TypeResult<T> = Result<T, TypeError>;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::terms::Term;
    use crate::types::Type;

    #[test]
    fn test_var() {
        let ctx = Context::new().with_term("x", Type::Nat);
        assert_eq!(type_of(&ctx, &Term::var("x")), Ok(Type::Nat));
    }

    #[test]
    fn test_unbound_var() {
        let ctx = Context::new();
        assert!(matches!(
            type_of(&ctx, &Term::var("x")),
            Err(TypeError::UnboundVariable(_))
        ));
    }

    #[test]
    fn test_lambda() {
        let ctx = Context::new();
        let id = Term::lambda("x", Type::Nat, Term::var("x"));
        assert_eq!(type_of(&ctx, &id), Ok(Type::arrow(Type::Nat, Type::Nat)));
    }

    #[test]
    fn test_application() {
        let ctx = Context::new();
        let id = Term::lambda("x", Type::Nat, Term::var("x"));
        let app = Term::app(id, Term::Zero);
        assert_eq!(type_of(&ctx, &app), Ok(Type::Nat));
    }

    #[test]
    fn test_application_type_mismatch() {
        let ctx = Context::new();
        let f = Term::lambda("x", Type::Nat, Term::var("x"));
        let app = Term::app(f, Term::True);
        assert!(matches!(
            type_of(&ctx, &app),
            Err(TypeError::ArgumentTypeMismatch { .. })
        ));
    }

    #[test]
    fn test_if() {
        let ctx = Context::new();
        let term = Term::if_then_else(Term::True, Term::Zero, Term::succ(Term::Zero));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    #[test]
    fn test_if_branch_mismatch() {
        let ctx = Context::new();
        let term = Term::if_then_else(Term::True, Term::Zero, Term::True);
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::BranchTypeMismatch { .. })
        ));
    }

    #[test]
    fn test_pair() {
        let ctx = Context::new();
        let pair = Term::pair(Term::Zero, Term::True);
        assert_eq!(
            type_of(&ctx, &pair),
            Ok(Type::product(Type::Nat, Type::Bool))
        );
    }

    #[test]
    fn test_fst() {
        let ctx = Context::new();
        let pair = Term::pair(Term::Zero, Term::True);
        let fst = Term::fst(pair);
        assert_eq!(type_of(&ctx, &fst), Ok(Type::Nat));
    }

    #[test]
    fn test_snd() {
        let ctx = Context::new();
        let pair = Term::pair(Term::Zero, Term::True);
        let snd = Term::snd(pair);
        assert_eq!(type_of(&ctx, &snd), Ok(Type::Bool));
    }

    #[test]
    fn test_sum_inl() {
        let ctx = Context::new();
        let sum_ty = Type::sum(Type::Nat, Type::Bool);
        let inl = Term::inl(sum_ty.clone(), Term::Zero);
        assert_eq!(type_of(&ctx, &inl), Ok(sum_ty));
    }

    #[test]
    fn test_sum_inr() {
        let ctx = Context::new();
        let sum_ty = Type::sum(Type::Nat, Type::Bool);
        let inr = Term::inr(sum_ty.clone(), Term::True);
        assert_eq!(type_of(&ctx, &inr), Ok(sum_ty));
    }

    #[test]
    fn test_case() {
        let ctx = Context::new();
        let sum_ty = Type::sum(Type::Nat, Type::Bool);
        let scrut = Term::inl(sum_ty, Term::Zero);
        let case = Term::case(
            scrut,
            "n",
            Term::var("n"), // : Nat
            "b",
            Term::if_then_else(Term::var("b"), Term::Zero, Term::succ(Term::Zero)), // : Nat
        );
        assert_eq!(type_of(&ctx, &case), Ok(Type::Nat));
    }

    #[test]
    fn test_polymorphism() {
        let ctx = Context::new();
        // Λα. λx:α. x : ∀α. α → α
        let poly_id = Term::ty_abs(
            "α",
            Term::lambda("x", Type::TyVar("α".into()), Term::var("x")),
        );
        let expected = Type::forall(
            "α",
            Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into())),
        );
        assert_eq!(type_of(&ctx, &poly_id), Ok(expected));
    }

    #[test]
    fn test_type_application() {
        let ctx = Context::new();
        // (Λα. λx:α. x) [Nat] : Nat → Nat
        let poly_id = Term::ty_abs(
            "α",
            Term::lambda("x", Type::TyVar("α".into()), Term::var("x")),
        );
        let instantiated = Term::ty_app(poly_id, Type::Nat);
        assert_eq!(
            type_of(&ctx, &instantiated),
            Ok(Type::arrow(Type::Nat, Type::Nat))
        );
    }

    #[test]
    fn test_let() {
        let ctx = Context::new();
        // let x : Nat = zero in succ x
        let term = Term::let_in("x", Type::Nat, Term::Zero, Term::succ(Term::var("x")));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    #[test]
    fn test_natrec() {
        let ctx = Context::new();
        // natrec [Nat] zero (λn. λacc. succ acc) (succ zero)
        // This computes: succ zero
        let term = Term::natrec(
            Type::Nat,
            Term::Zero,
            Term::lambda(
                "n",
                Type::Nat,
                Term::lambda("acc", Type::Nat, Term::succ(Term::var("acc"))),
            ),
            Term::succ(Term::Zero),
        );
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    #[test]
    fn test_refl() {
        let ctx = Context::new();
        // refl [Nat] zero : Eq Nat zero zero
        let term = Term::refl(Type::Nat, Term::Zero);
        let expected = Type::eq(Type::Nat, Term::Zero, Term::Zero);
        assert_eq!(type_of(&ctx, &term), Ok(expected));
    }

    #[test]
    fn test_unit() {
        let ctx = Context::new();
        assert_eq!(type_of(&ctx, &Term::Unit), Ok(Type::Unit));
    }

    #[test]
    fn test_absurd() {
        // We can't actually construct a term of type Void,
        // but we can check the typing rule with a variable
        let ctx = Context::new().with_term("x", Type::Void);
        let term = Term::absurd(Type::Nat, Term::var("x"));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    // ==========================================================================
    // Phase 3-Prep Tests: Integer Comparison
    // ==========================================================================

    #[test]
    fn test_nat_lt() {
        let ctx = Context::new();
        let term = Term::nat_lt(Term::Zero, Term::succ(Term::Zero));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Bool));
    }

    #[test]
    fn test_nat_le() {
        let ctx = Context::new();
        let term = Term::nat_le(Term::Zero, Term::Zero);
        assert_eq!(type_of(&ctx, &term), Ok(Type::Bool));
    }

    #[test]
    fn test_nat_gt() {
        let ctx = Context::new();
        let term = Term::nat_gt(Term::succ(Term::Zero), Term::Zero);
        assert_eq!(type_of(&ctx, &term), Ok(Type::Bool));
    }

    #[test]
    fn test_nat_ge() {
        let ctx = Context::new();
        let term = Term::nat_ge(Term::Zero, Term::Zero);
        assert_eq!(type_of(&ctx, &term), Ok(Type::Bool));
    }

    #[test]
    fn test_nat_comparison_type_mismatch() {
        let ctx = Context::new();
        let term = Term::nat_lt(Term::True, Term::Zero);
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::TypeMismatch { .. })
        ));
    }

    // ==========================================================================
    // Phase 3-Prep Tests: String char_at
    // ==========================================================================

    #[test]
    fn test_str_char_at() {
        let ctx = Context::new();
        let term = Term::str_char_at(Term::string_lit("hello"), Term::Zero);
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    #[test]
    fn test_str_char_at_type_mismatch_string() {
        let ctx = Context::new();
        let term = Term::str_char_at(Term::Zero, Term::Zero); // Nat instead of String
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn test_str_char_at_type_mismatch_index() {
        let ctx = Context::new();
        let term = Term::str_char_at(Term::string_lit("hi"), Term::True); // Bool instead of Nat
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::TypeMismatch { .. })
        ));
    }

    // ==========================================================================
    // Phase 3-Prep Tests: Ref Cells
    // ==========================================================================

    #[test]
    fn test_ref_new() {
        let ctx = Context::new();
        // Use NatLit instead of nat(42) to avoid deeply nested Succ terms that overflow the stack
        let term = Term::ref_new(Term::NatLit(42));
        assert_eq!(type_of(&ctx, &term), Ok(Type::ref_ty(Type::Nat)));
    }

    #[test]
    fn test_ref_get() {
        let ctx = Context::new().with_term("r", Type::ref_ty(Type::Nat));
        let term = Term::ref_get(Term::var("r"));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
    }

    #[test]
    fn test_ref_get_type_mismatch() {
        let ctx = Context::new().with_term("x", Type::Nat);
        let term = Term::ref_get(Term::var("x")); // Nat is not Ref<_>
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn test_ref_set() {
        let ctx = Context::new().with_term("r", Type::ref_ty(Type::Nat));
        let term = Term::ref_set(Term::var("r"), Term::nat(10));
        assert_eq!(type_of(&ctx, &term), Ok(Type::Unit));
    }

    #[test]
    fn test_ref_set_type_mismatch() {
        let ctx = Context::new().with_term("r", Type::ref_ty(Type::Nat));
        let term = Term::ref_set(Term::var("r"), Term::True); // Bool instead of Nat
        assert!(matches!(
            type_of(&ctx, &term),
            Err(TypeError::TypeMismatch { .. })
        ));
    }

    // ==========================================================================
    // Phase 3-Prep Tests: Ptr and Ref Types
    // ==========================================================================

    #[test]
    fn test_ptr_type_well_formed() {
        let ty = Type::ptr(Type::Nat);
        let type_vars = std::collections::HashSet::new();
        assert!(ty.is_well_formed(&type_vars));
    }

    #[test]
    fn test_ref_type_well_formed() {
        let ty = Type::ref_ty(Type::String);
        let type_vars = std::collections::HashSet::new();
        assert!(ty.is_well_formed(&type_vars));
    }

    #[test]
    fn test_ptr_type_display() {
        let ty = Type::ptr(Type::Nat);
        assert_eq!(ty.to_string(), "Ptr<Nat>");
    }

    #[test]
    fn test_ref_type_display() {
        let ty = Type::ref_ty(Type::Bool);
        assert_eq!(ty.to_string(), "Ref<Bool>");
    }
}
