//! Core substitution tests.

use crate::terms::tests::*;
#[test]
fn test_substitute_fix_shadowing() {
    let fix = Term::Fix("f".into(), Type::Nat, Box::new(Term::var("f")));
    let result = fix.substitute("f", &Term::Zero);
    assert_eq!(result, fix);
}

#[test]
fn test_substitute_adt_match_shadowing() {
    let term = Term::adt_match(
        Term::var("scrut"),
        vec![(0, "x".into(), Box::new(Term::var("x")))],
    );
    let result = term.substitute("x", &Term::Zero);
    match &result {
        Term::AdtMatch(scrut, arms) => {
            assert_eq!(**scrut, Term::var("scrut"));
            assert_eq!(arms[0].2.as_ref(), &Term::var("x"));
        }
        _ => panic!("expected AdtMatch"),
    }
}

#[test]
fn test_substitute_nat_binop() {
    let term = Term::NatAdd(Box::new(Term::var("x")), Box::new(Term::var("y")));
    let one = Term::succ(Term::Zero);
    let result = term.substitute("x", &one);
    assert_eq!(
        result,
        Term::NatAdd(Box::new(one), Box::new(Term::var("y")))
    );
}

#[test]
fn test_substitute_type_in_lambda() {
    let term = Term::lambda("x", Type::TyVar("α".into()), Term::var("x"));
    let result = term.substitute_type("α", &Type::Nat);
    assert_eq!(result, Term::lambda("x", Type::Nat, Term::var("x")));
}

#[test]
fn test_substitute_type_tyabs_shadowing() {
    let term = Term::ty_abs("α", Term::var("x"));
    let result = term.substitute_type("α", &Type::Nat);
    assert_eq!(result, term);
}
