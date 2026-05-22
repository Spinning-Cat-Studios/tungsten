use super::*;
use crate::codegen::CodeGen;
use inkwell::context::Context;

fn setup_codegen(context: &Context) -> CodeGen {
    CodeGen::new(context, "test")
}

#[test]
fn test_infer_bool() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    assert_eq!(codegen.infer_term_type(&Term::True).unwrap(), Type::Bool);
    assert_eq!(codegen.infer_term_type(&Term::False).unwrap(), Type::Bool);
}

#[test]
fn test_infer_nat() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    assert_eq!(codegen.infer_term_type(&Term::Zero).unwrap(), Type::Nat);
    assert_eq!(
        codegen
            .infer_term_type(&Term::Succ(Box::new(Term::Zero)))
            .unwrap(),
        Type::Nat
    );
    assert_eq!(
        codegen.infer_term_type(&Term::NatLit(42)).unwrap(),
        Type::Nat
    );
}

#[test]
fn test_infer_unit() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    assert_eq!(codegen.infer_term_type(&Term::Unit).unwrap(), Type::Unit);
}

#[test]
fn test_infer_string() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    assert_eq!(
        codegen
            .infer_term_type(&Term::StringLit("hello".to_string()))
            .unwrap(),
        Type::String
    );
}

#[test]
fn test_infer_pair() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let pair = Term::Pair(Box::new(Term::True), Box::new(Term::Zero));
    let ty = codegen.infer_term_type(&pair).unwrap();
    assert_eq!(ty, Type::product(Type::Bool, Type::Nat));
}

#[test]
fn test_infer_lambda() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let lambda = Term::Lambda(
        "x".to_string(),
        Type::Nat,
        Box::new(Term::Var("x".to_string())),
    );
    let ty = codegen.infer_term_type(&lambda).unwrap();
    assert_eq!(ty, Type::arrow(Type::Nat, Type::Nat));
}

#[test]
fn test_infer_let() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let let_term = Term::Let(
        "x".to_string(),
        Type::Nat,
        Box::new(Term::NatLit(42)),
        Box::new(Term::Var("x".to_string())),
    );
    let ty = codegen.infer_term_type(&let_term).unwrap();
    assert_eq!(ty, Type::Nat);
}

#[test]
fn test_infer_nat_ops() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let add = Term::NatAdd(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
    assert_eq!(codegen.infer_term_type(&add).unwrap(), Type::Nat);

    let eq = Term::NatEq(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
    assert_eq!(codegen.infer_term_type(&eq).unwrap(), Type::Bool);

    let lt = Term::NatLt(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
    assert_eq!(codegen.infer_term_type(&lt).unwrap(), Type::Bool);
}

#[test]
fn test_infer_bool_ops() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let and = Term::BoolAnd(Box::new(Term::True), Box::new(Term::False));
    assert_eq!(codegen.infer_term_type(&and).unwrap(), Type::Bool);

    let not = Term::BoolNot(Box::new(Term::True));
    assert_eq!(codegen.infer_term_type(&not).unwrap(), Type::Bool);
}

#[test]
fn test_infer_annot() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let annot = Term::Annot(Box::new(Term::NatLit(42)), Type::Nat);
    assert_eq!(codegen.infer_term_type(&annot).unwrap(), Type::Nat);
}

#[test]
fn test_infer_unbound_var() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let result = codegen.infer_term_type(&Term::Var("x".to_string()));
    assert!(result.is_err());
}
