use super::*;
use crate::eval::{eval, eval_with_limit};
use crate::types::Type;

#[test]
fn test_beta_reduction() {
    // (λx:Nat. x) zero → zero
    let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_if_true() {
    let term = Term::if_then_else(Term::True, Term::Zero, Term::succ(Term::Zero));
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_if_false() {
    let term = Term::if_then_else(Term::False, Term::Zero, Term::succ(Term::Zero));
    let result = eval(&term);
    assert_eq!(result, Term::succ(Term::Zero));
}

#[test]
fn test_pair_fst() {
    let term = Term::fst(Term::pair(Term::Zero, Term::succ(Term::Zero)));
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_pair_snd() {
    let term = Term::snd(Term::pair(Term::Zero, Term::succ(Term::Zero)));
    let result = eval(&term);
    assert_eq!(result, Term::succ(Term::Zero));
}

#[test]
fn test_let() {
    let term = Term::let_in("x", Type::Nat, Term::Zero, Term::succ(Term::var("x")));
    let result = eval(&term);
    assert_eq!(result, Term::succ(Term::Zero));
}

#[test]
fn test_natrec_zero() {
    let term = Term::natrec(
        Type::Nat,
        Term::succ(Term::Zero),
        Term::lambda(
            "_",
            Type::Nat,
            Term::lambda("acc", Type::Nat, Term::var("acc")),
        ),
        Term::Zero,
    );
    let result = eval(&term);
    assert_eq!(result, Term::succ(Term::Zero));
}

#[test]
fn test_natrec_succ() {
    let term = Term::natrec(
        Type::Nat,
        Term::Zero,
        Term::lambda(
            "_",
            Type::Nat,
            Term::lambda("acc", Type::Nat, Term::succ(Term::var("acc"))),
        ),
        Term::succ(Term::succ(Term::Zero)),
    );
    let result = eval(&term);
    assert_eq!(result, Term::nat(2));
}

#[test]
fn test_case_inl() {
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let term = Term::case(
        Term::inl(sum_ty, Term::Zero),
        "x",
        Term::succ(Term::var("x")),
        "_",
        Term::Zero,
    );
    let result = eval(&term);
    assert_eq!(result, Term::succ(Term::Zero));
}

#[test]
fn test_case_inr() {
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let term = Term::case(
        Term::inr(sum_ty, Term::True),
        "_",
        Term::False,
        "b",
        Term::var("b"),
    );
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_type_application() {
    let poly_id = Term::ty_abs(
        "α",
        Term::lambda("x", Type::TyVar("α".into()), Term::var("x")),
    );
    let term = Term::app(Term::ty_app(poly_id, Type::Nat), Term::Zero);
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_subst_refl() {
    let motive = Type::arrow(Type::Nat, Type::Prop);
    let term = Term::subst(
        Type::Nat,
        motive,
        Term::refl(Type::Nat, Term::Zero),
        Term::Unit,
    );
    let result = eval(&term);
    assert_eq!(result, Term::Unit);
}

#[test]
fn test_nested_application() {
    let double_apply = Term::lambda(
        "f",
        Type::arrow(Type::Nat, Type::Nat),
        Term::lambda(
            "x",
            Type::Nat,
            Term::app(Term::var("f"), Term::app(Term::var("f"), Term::var("x"))),
        ),
    );
    let succ_fn = Term::lambda("n", Type::Nat, Term::succ(Term::var("n")));
    let term = Term::app(Term::app(double_apply, succ_fn), Term::Zero);
    let result = eval(&term);
    assert_eq!(result, Term::nat(2));
}

#[test]
fn test_sorry_stuck() {
    let result = step(&Term::Sorry);
    assert_eq!(result, StepResult::Stuck);
}

#[test]
fn test_eval_with_limit() {
    let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
    let result = eval_with_limit(&term, 100);
    assert_eq!(result, Some(Term::Zero));
}
