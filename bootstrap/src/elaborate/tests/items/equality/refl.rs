//! Phase 1 tests: `refl` checking against equality types.

use super::*;

#[test]
fn test_refl_zero_eq_zero() {
    let ty = Type::eq(Type::Nat, Term::Zero, Term::Zero);
    let result = check_refl(&ty);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert_eq!(result.unwrap(), Term::refl(Type::Nat, Term::Zero));
}

#[test]
fn test_refl_true_eq_true() {
    let ty = Type::eq(Type::Bool, Term::True, Term::True);
    let result = check_refl(&ty);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[test]
fn test_refl_definitional_equality_arithmetic() {
    // 2 + 3 and 5 should normalize to the same Nat value.
    let two = Term::succ(Term::succ(Term::Zero));
    let three = Term::succ(Term::succ(Term::succ(Term::Zero)));
    let five = Term::succ(Term::succ(Term::succ(Term::succ(Term::succ(Term::Zero)))));
    let sum = Term::NatAdd(Box::new(two), Box::new(three));
    let ty = Type::eq(Type::Nat, sum, five);
    let result = check_refl(&ty);
    assert!(result.is_ok(), "expected Ok for 2+3==5, got {:?}", result);
}

#[test]
fn test_refl_unequal_sides_produces_invalid_refl() {
    let ty = Type::eq(Type::Nat, Term::Zero, Term::succ(Term::Zero));
    let result = check_refl(&ty);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ElabErrorKind::InvalidRefl { .. }),
        "expected InvalidRefl"
    );
}

#[test]
fn test_refl_against_nat_produces_refl_expected_equality() {
    let result = check_refl(&Type::Nat);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ElabErrorKind::ReflExpectedEquality(_)),
        "expected ReflExpectedEquality"
    );
}

#[test]
fn test_refl_against_bool_produces_refl_expected_equality() {
    let result = check_refl(&Type::Bool);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ElabErrorKind::ReflExpectedEquality(_)),
        "expected ReflExpectedEquality"
    );
}

#[test]
fn test_refl_infer_mode_cannot_infer() {
    use crate::elaborate::tests::elab_err;
    let errors = elab_err(
        r#"
        fn test() -> Bool {
            let x = refl;
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(
        matches!(errors[0].kind, ElabErrorKind::CannotInferType),
        "expected CannotInferType, got {:?}",
        errors[0].kind
    );
}
