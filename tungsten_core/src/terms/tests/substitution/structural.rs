//! Core structural helper substitution tests (unary, typed-unary, binary).

use crate::terms::tests::*;
#[test]
fn test_substitute_core_unary() {
    assert_eq!(
        Term::succ(Term::var("x")).substitute("x", &Term::Zero),
        Term::succ(Term::Zero),
    );
    let p = Term::pair(Term::var("a"), Term::var("b"));
    assert_eq!(
        Term::fst(Term::var("x")).substitute("x", &p),
        Term::fst(p.clone()),
    );
}

#[test]
fn test_substitute_core_typed_unary() {
    assert_eq!(
        Term::inl(Type::Bool, Term::var("x")).substitute("x", &Term::Zero),
        Term::inl(Type::Bool, Term::Zero),
    );
}

#[test]
fn test_substitute_core_binary() {
    let id = Term::lambda("z", Type::Nat, Term::var("z"));
    assert_eq!(
        Term::app(Term::var("f"), Term::var("x")).substitute("f", &id),
        Term::app(id.clone(), Term::var("x")),
    );
}

#[test]
fn test_substitute_type_core_unary() {
    assert_eq!(
        Term::succ(Term::var("x")).substitute_type("α", &Type::Nat),
        Term::succ(Term::var("x")),
    );
}

#[test]
fn test_substitute_type_core_typed_unary() {
    assert_eq!(
        Term::inl(Type::TyVar("α".into()), Term::var("x")).substitute_type("α", &Type::Nat),
        Term::inl(Type::Nat, Term::var("x")),
    );
}
