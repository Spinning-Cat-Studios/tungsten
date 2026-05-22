//! Variable shadowing substitution tests.

use crate::terms::tests::*;
#[test]
fn test_substitute_lambda_shadows() {
    let term = Term::lambda("x", Type::Nat, Term::var("x"));
    let result = term.substitute("x", &Term::Zero);
    assert_eq!(result, Term::lambda("x", Type::Nat, Term::var("x")));
}

#[test]
fn test_substitute_lambda_no_shadow() {
    let term = Term::lambda("x", Type::Nat, Term::var("y"));
    let result = term.substitute("y", &Term::Zero);
    assert_eq!(result, Term::lambda("x", Type::Nat, Term::Zero));
}

#[test]
fn test_substitute_let_shadows() {
    let term = Term::let_in("x", Type::Nat, Term::succ(Term::var("y")), Term::var("x"));
    let result = term.substitute("x", &Term::Zero);
    assert_eq!(
        result,
        Term::let_in("x", Type::Nat, Term::succ(Term::var("y")), Term::var("x"))
    );
}

#[test]
fn test_substitute_case_shadows() {
    let ctx_var = Term::var("z");
    let term = Term::case(ctx_var, "x", Term::var("x"), "y", Term::var("y"));
    let result = term.substitute("x", &Term::Zero);
    assert_eq!(
        result,
        Term::case(Term::var("z"), "x", Term::var("x"), "y", Term::var("y"))
    );
}

#[test]
fn test_substitute_type_tyabs_shadows() {
    let inner = Term::lambda("x", Type::TyVar("α".into()), Term::var("x"));
    let term = Term::ty_abs("α", inner.clone());
    let result = term.substitute_type("α", &Type::Nat);
    assert_eq!(result, Term::ty_abs("α", inner));
}

#[test]
fn test_substitute_type_tyabs_no_shadow() {
    let inner = Term::lambda("x", Type::TyVar("β".into()), Term::var("x"));
    let term = Term::ty_abs("α", inner);
    let result = term.substitute_type("β", &Type::Nat);
    assert_eq!(
        result,
        Term::ty_abs("α", Term::lambda("x", Type::Nat, Term::var("x")))
    );
}
