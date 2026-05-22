use super::*;
use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

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
