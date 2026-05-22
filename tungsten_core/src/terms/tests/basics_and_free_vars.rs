use super::*;

#[test]
fn test_nat_literal() {
    assert_eq!(Term::nat(0), Term::Zero);
    assert_eq!(Term::nat(1), Term::succ(Term::Zero));
    assert_eq!(Term::nat(2), Term::succ(Term::succ(Term::Zero)));
}

#[test]
fn test_is_value() {
    assert!(Term::lambda("x", Type::Nat, Term::var("x")).is_value());
    assert!(Term::True.is_value());
    assert!(Term::False.is_value());
    assert!(Term::Zero.is_value());
    assert!(Term::succ(Term::Zero).is_value());
    assert!(Term::Unit.is_value());
    assert!(Term::pair(Term::Zero, Term::True).is_value());
    assert!(!Term::app(Term::var("f"), Term::var("x")).is_value());
    assert!(!Term::fst(Term::pair(Term::Zero, Term::True)).is_value());
}

#[test]
fn test_term_substitution() {
    let id = Term::lambda("x", Type::Nat, Term::var("x"));
    let result = id.substitute("x", &Term::Zero);
    assert_eq!(result, id);

    let y = Term::var("y");
    let result = y.substitute("y", &Term::Zero);
    assert_eq!(result, Term::Zero);

    let term = Term::lambda("x", Type::Nat, Term::var("y"));
    let result = term.substitute("y", &Term::Zero);
    assert_eq!(result, Term::lambda("x", Type::Nat, Term::Zero));
}

#[test]
fn test_free_vars() {
    let id = Term::lambda("x", Type::Nat, Term::var("x"));
    assert!(id.free_vars().is_empty());

    let open = Term::lambda("x", Type::Nat, Term::var("y"));
    assert!(open.free_vars().contains("y"));
    assert!(!open.free_vars().contains("x"));

    let app = Term::app(Term::var("f"), Term::var("x"));
    assert!(app.free_vars().contains("f"));
    assert!(app.free_vars().contains("x"));
}

#[test]
fn test_display() {
    let id = Term::lambda("x", Type::Nat, Term::var("x"));
    assert_eq!(id.to_string(), "(λx:Nat. x)");

    let app = Term::app(Term::var("f"), Term::Zero);
    assert_eq!(app.to_string(), "(f zero)");
}

#[test]
fn test_free_vars_binary_ops() {
    let add = Term::NatAdd(Box::new(Term::var("x")), Box::new(Term::var("y")));
    let fv = add.free_vars();
    assert!(fv.contains("x"));
    assert!(fv.contains("y"));
    assert_eq!(fv.len(), 2);

    let cat = Term::StrConcat(Box::new(Term::var("a")), Box::new(Term::var("b")));
    let fv = cat.free_vars();
    assert!(fv.contains("a"));
    assert!(fv.contains("b"));
}

#[test]
fn test_free_vars_fix_binding() {
    let fix = Term::Fix(
        "f".into(),
        Type::Nat,
        Box::new(Term::app(Term::var("f"), Term::var("x"))),
    );
    let fv = fix.free_vars();
    assert!(!fv.contains("f"));
    assert!(fv.contains("x"));
}

#[test]
fn test_free_type_vars_lambda() {
    let term = Term::lambda("x", Type::TyVar("α".into()), Term::var("x"));
    let ftv = term.free_type_vars();
    assert!(ftv.contains("α"));
}

#[test]
fn test_free_type_vars_tyabs_binding() {
    let inner = Term::lambda("x", Type::TyVar("α".into()), Term::var("x"));
    let term = Term::ty_abs("α", inner);
    let ftv = term.free_type_vars();
    assert!(ftv.is_empty());
}

#[test]
fn test_free_type_vars_binary_ops() {
    let add = Term::NatAdd(Box::new(Term::var("x")), Box::new(Term::var("y")));
    assert!(add.free_type_vars().is_empty());
}
