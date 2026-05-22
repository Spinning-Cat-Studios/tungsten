//! Fix-point binding substitution tests.

use crate::terms::tests::*;
#[test]
fn test_substitute_fix_binding() {
    let fix = Term::Fix(
        "f".into(),
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        Box::new(Term::lambda(
            "n",
            Type::Nat,
            Term::NatAdd(Box::new(Term::var("n")), Box::new(Term::var("x"))),
        )),
    );
    let result = fix.substitute("x", &Term::succ(Term::Zero));
    let Term::Fix(_, _, body) = &result else {
        panic!("expected Fix");
    };
    let Term::Lambda(_, _, body) = body.as_ref() else {
        panic!("expected Lambda");
    };
    let Term::NatAdd(_, rhs) = body.as_ref() else {
        panic!("expected NatAdd");
    };
    assert_eq!(rhs.as_ref(), &Term::succ(Term::Zero));
}

#[test]
fn test_substitute_type_fix() {
    let fix = Term::Fix(
        "f".into(),
        Type::Arrow(
            Box::new(Type::TyVar("α".into())),
            Box::new(Type::TyVar("α".into())),
        ),
        Box::new(Term::lambda("x", Type::TyVar("α".into()), Term::var("x"))),
    );
    let result = fix.substitute_type("α", &Type::Nat);
    match &result {
        Term::Fix(_, ty, _) => {
            assert_eq!(*ty, Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)));
        }
        _ => panic!("expected Fix"),
    }
}
