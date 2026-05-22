use super::*;

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
fn test_natrec() {
    let ctx = Context::new();
    // natrec [Nat] zero (λn. λacc. succ acc) (succ zero)
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
fn test_succ_not_nat() {
    let ctx = Context::new();
    let term = Term::succ(Term::True);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotANat { .. })
    ));
}

#[test]
fn test_absurd() {
    let ctx = Context::new().with_term("x", Type::Void);
    let term = Term::absurd(Type::Nat, Term::var("x"));
    assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
}

#[test]
fn test_absurd_not_void() {
    let ctx = Context::new();
    let term = Term::absurd(Type::Nat, Term::Zero);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotVoid { .. })
    ));
}
