use super::*;
use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

// ==========================================================================
// Core typing rules error paths (rules_core.rs)
// ==========================================================================

#[test]
fn test_app_not_a_function() {
    let ctx = Context::new();
    let app = Term::app(Term::Zero, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &app),
        Err(TypeError::NotAFunction { .. })
    ));
}

#[test]
fn test_let_type_mismatch() {
    let ctx = Context::new();
    // let x : Bool = zero in x — definition doesn't match declared type
    let term = Term::let_in("x", Type::Bool, Term::Zero, Term::var("x"));
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_if_condition_not_bool() {
    let ctx = Context::new();
    let term = Term::if_then_else(Term::Zero, Term::Zero, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::ConditionNotBool { .. })
    ));
}

#[test]
fn test_natrec_zero_case_mismatch() {
    let ctx = Context::new();
    // natrec [Nat] true (λn. λacc. acc) zero — zero case is Bool, not Nat
    let term = Term::natrec(
        Type::Nat,
        Term::True,
        Term::lambda(
            "n",
            Type::Nat,
            Term::lambda("acc", Type::Nat, Term::var("acc")),
        ),
        Term::Zero,
    );
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_natrec_n_not_nat() {
    let ctx = Context::new();
    // natrec [Nat] zero (λn. λacc. acc) true — n is Bool, not Nat
    let term = Term::natrec(
        Type::Nat,
        Term::Zero,
        Term::lambda(
            "n",
            Type::Nat,
            Term::lambda("acc", Type::Nat, Term::var("acc")),
        ),
        Term::True,
    );
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotANat { .. })
    ));
}

#[test]
fn test_inl_type_mismatch() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    // inl [Nat + Bool] true — injecting Bool into left (expects Nat)
    let inl = Term::inl(sum_ty, Term::True);
    assert!(matches!(
        type_of(&ctx, &inl),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_inl_not_a_sum() {
    let ctx = Context::new();
    let inl = Term::inl(Type::Nat, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &inl),
        Err(TypeError::NotASum { .. })
    ));
}

#[test]
fn test_inr_type_mismatch() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    // inr [Nat + Bool] zero — injecting Nat into right (expects Bool)
    let inr = Term::inr(sum_ty, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &inr),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_case_branch_type_mismatch() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let scrut = Term::inl(sum_ty, Term::Zero);
    // case: left branch returns Nat, right branch returns Bool
    let case = Term::case(
        scrut,
        "n",
        Term::var("n"), // : Nat
        "b",
        Term::var("b"), // : Bool — mismatch
    );
    assert!(matches!(
        type_of(&ctx, &case),
        Err(TypeError::BranchTypeMismatch { .. })
    ));
}

#[test]
fn test_case_not_a_sum() {
    let ctx = Context::new();
    let case = Term::case(
        Term::Zero, // Nat, not Sum
        "x",
        Term::var("x"),
        "y",
        Term::var("y"),
    );
    assert!(matches!(
        type_of(&ctx, &case),
        Err(TypeError::NotASum { .. })
    ));
}

#[test]
fn test_tyapp_not_polymorphic() {
    let ctx = Context::new();
    // zero [Nat] — trying to apply type to non-polymorphic term
    let term = Term::ty_app(Term::Zero, Type::Nat);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotPolymorphic { .. })
    ));
}

#[test]
fn test_refl_type_mismatch() {
    let ctx = Context::new();
    // refl [Bool] zero — zero has type Nat, not Bool
    let term = Term::refl(Type::Bool, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_annot_type_mismatch() {
    let ctx = Context::new();
    // (zero : Bool) — annotation type doesn't match term type
    let term = Term::annot(Term::Zero, Type::Bool);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::TypeMismatch { .. })
    ));
}

#[test]
fn test_annot_sorry_accepts_any_type() {
    let ctx = Context::new();
    // (sorry : Nat → Bool) — sorry accepts any annotated type
    let term = Term::annot(Term::Sorry, Type::arrow(Type::Nat, Type::Bool));
    assert_eq!(type_of(&ctx, &term), Ok(Type::arrow(Type::Nat, Type::Bool)));
}
