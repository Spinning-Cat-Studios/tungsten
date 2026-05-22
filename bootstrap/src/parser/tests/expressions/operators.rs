//! Operator expression parsing tests.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_binary_ops() {
    let e = parse_expr_ok("1 + 2 * 3");
    match e {
        Expr::Binary(left, BinOp::Add, right, _) => {
            assert!(matches!(*left, Expr::IntLiteral(1, _)));
            assert!(matches!(*right, Expr::Binary(_, BinOp::Mul, _, _)));
        }
        _ => panic!("Expected binary expression"),
    }
}

#[test]
fn test_comparison() {
    let e = parse_expr_ok("x == y");
    match e {
        Expr::Binary(_, BinOp::Eq, _, _) => {}
        _ => panic!("Expected equality"),
    }
}

#[test]
fn test_logical_ops() {
    let e = parse_expr_ok("a && b || c");
    match e {
        Expr::Binary(_, BinOp::Or, _, _) => {}
        _ => panic!("Expected or expression"),
    }
}

#[test]
fn test_unary_not() {
    let e = parse_expr_ok("!x");
    match e {
        Expr::Unary(UnaryOp::Not, _, _) => {}
        _ => panic!("Expected not expression"),
    }
}

#[test]
fn test_unary_neg() {
    let e = parse_expr_ok("-42");
    match e {
        Expr::Unary(UnaryOp::Neg, _, _) => {}
        _ => panic!("Expected negation"),
    }
}

#[test]
fn test_type_annotation() {
    let e = parse_expr_ok("x : Nat");
    match e {
        Expr::Annot(_, _, _) => {}
        _ => panic!("Expected annotation"),
    }
}
