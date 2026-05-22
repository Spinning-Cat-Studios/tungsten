//! Extension operation substitution tests (binary/unary ops).

use crate::terms::tests::*;
#[test]
fn test_substitute_ext_binary_nat_add() {
    let term = Term::NatAdd(Box::new(Term::var("x")), Box::new(Term::var("y")));
    let result = term.substitute("x", &Term::NatLit(1));
    assert_eq!(
        result,
        Term::NatAdd(Box::new(Term::NatLit(1)), Box::new(Term::var("y")))
    );
}

#[test]
fn test_substitute_ext_binary_bool_or() {
    let term = Term::BoolOr(Box::new(Term::var("x")), Box::new(Term::var("y")));
    let result = term.substitute("y", &Term::True);
    assert_eq!(
        result,
        Term::BoolOr(Box::new(Term::var("x")), Box::new(Term::True))
    );
}

#[test]
fn test_substitute_ext_unary_bool_not() {
    let term = Term::bool_not(Term::var("x"));
    let result = term.substitute("x", &Term::True);
    assert_eq!(result, Term::bool_not(Term::True));
}

#[test]
fn test_substitute_ext_unary_str_len() {
    let term = Term::str_len(Term::var("x"));
    let result = term.substitute("x", &Term::string_lit("hi"));
    assert_eq!(result, Term::str_len(Term::string_lit("hi")));
}

#[test]
fn test_substitute_ext_fix_shadows() {
    let term = Term::Fix("f".into(), Type::Nat, Box::new(Term::var("f")));
    let result = term.substitute("f", &Term::Zero);
    assert_eq!(
        result,
        Term::Fix("f".into(), Type::Nat, Box::new(Term::var("f")))
    );
}

#[test]
fn test_substitute_type_ext_binary_nat_mul() {
    let term = Term::NatMul(Box::new(Term::var("x")), Box::new(Term::var("y")));
    let result = term.substitute_type("α", &Type::Nat);
    assert_eq!(result, term);
}

#[test]
fn test_substitute_type_ext_fold() {
    let mu_ty = Type::Mu("α".into(), Box::new(Type::TyVar("α".into())));
    let term = Term::fold(mu_ty.clone(), Term::var("x"));
    let result = term.substitute_type("β", &Type::Nat);
    assert_eq!(result, Term::fold(mu_ty, Term::var("x")));
}
