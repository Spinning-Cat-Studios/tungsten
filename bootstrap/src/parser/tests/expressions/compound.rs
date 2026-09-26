//! Compound expression parsing tests (calls, control flow, tuples, lambdas).

use crate::ast::*;
use crate::parser::tests::{parse_expr_ok, parse_has_errors, parse_ok, unwrap_fn};

#[test]
fn test_function_call() {
    let e = parse_expr_ok("foo(1, 2)");
    match e {
        Expr::App(func, args, _) => {
            assert!(matches!(*func, Expr::Path(_)));
            assert_eq!(args.len(), 2);
        }
        _ => panic!("Expected function call"),
    }
}

#[test]
fn test_if_expr() {
    let e = parse_expr_ok("if x { 1 } else { 2 }");
    match e {
        Expr::If(_, _, _, _) => {}
        _ => panic!("Expected if expression"),
    }
}

#[test]
fn test_match_expr() {
    let file = parse_ok("fn test() { match x { 0 => true, _ => false } }");
    let body = &unwrap_fn(&file).body;
    let Expr::Block(_, Some(e), _) = body else {
        panic!("Expected block with final expression");
    };
    let Expr::Match(_, arms, _) = e.as_ref() else {
        panic!("Expected match");
    };
    assert_eq!(arms.len(), 2);
}

/// 18.9.26e AC 1: `-1 | 7` parses as an or of a negative and a positive
/// literal, and the negative carries its magnitude.
#[test]
fn negative_literal_pattern_carries_its_magnitude() {
    let file = parse_ok("fn test() { match x { -1 | 7 => 0, -9223372036854775808 => 1, _ => 2 } }");
    let Expr::Block(_, Some(e), _) = &unwrap_fn(&file).body else {
        panic!("Expected block with final expression");
    };
    let Expr::Match(_, arms, _) = e.as_ref() else {
        panic!("Expected match");
    };
    let Pattern::Or(left, right, _) = &arms[0].pattern else {
        panic!("Expected or-pattern, got {:?}", arms[0].pattern);
    };
    assert!(matches!(
        **left,
        Pattern::Literal(LiteralPattern::NegInt(1, _))
    ));
    assert!(matches!(
        **right,
        Pattern::Literal(LiteralPattern::Int(7, _))
    ));
    assert!(matches!(
        arms[1].pattern,
        Pattern::Literal(LiteralPattern::NegInt(9_223_372_036_854_775_808, _))
    ));
}

/// 18.9.26e AC 1: only a literal may follow `-` in a pattern — a pattern is
/// not an expression, so `-y` is a parse error, not a negated binder.
#[test]
fn negative_pattern_requires_a_literal_after_the_minus() {
    assert!(parse_has_errors("match x { -y => 0, _ => 1 }"));
    assert!(!parse_has_errors("match x { -3 => 0, _ => 1 }"));
}

#[test]
fn test_tuple() {
    let e = parse_expr_ok("(1, 2, 3)");
    match e {
        Expr::Tuple(elements, _) => {
            assert_eq!(elements.len(), 3);
        }
        _ => panic!("Expected tuple"),
    }
}

#[test]
fn test_unit() {
    let e = parse_expr_ok("()");
    match e {
        Expr::Unit(_) => {}
        _ => panic!("Expected unit"),
    }
}

#[test]
fn test_lambda_pipe() {
    let e = parse_expr_ok("|x| x + 1");
    match e {
        Expr::Lambda(params, _, _) => {
            assert_eq!(params.len(), 1);
        }
        _ => panic!("Expected lambda"),
    }
}

#[test]
fn test_lambda_fn() {
    let e = parse_expr_ok("fn(x: Nat) => x");
    match e {
        Expr::Lambda(params, _, _) => {
            assert_eq!(params.len(), 1);
            assert!(params[0].ty.is_some());
        }
        _ => panic!("Expected lambda"),
    }
}
