use super::*;

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
fn test_unit() {
    let ctx = Context::new();
    assert_eq!(type_of(&ctx, &Term::Unit), Ok(Type::Unit));
}

#[test]
fn test_let() {
    let ctx = Context::new();
    // let x : Nat = zero in succ x
    let term = Term::let_in("x", Type::Nat, Term::Zero, Term::succ(Term::var("x")));
    assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
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
