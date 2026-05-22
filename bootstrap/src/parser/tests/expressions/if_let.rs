//! `if let` parsing tests — ADR 14.5.26e.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_if_let_basic() {
    let e = parse_expr_ok("if let Some(x) = opt { x } else { 0 }");
    match e {
        Expr::IfLet(pattern, _init, _body, else_branch, _) => {
            assert!(
                matches!(pattern, Pattern::Constructor(_, _, _)),
                "Expected constructor pattern, got {:?}",
                pattern
            );
            assert!(else_branch.is_some(), "Expected else branch");
        }
        _ => panic!("Expected IfLet expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_no_else() {
    let e = parse_expr_ok("if let Some(x) = opt { use_x(x) }");
    match e {
        Expr::IfLet(pattern, _init, _body, else_branch, _) => {
            assert!(
                matches!(pattern, Pattern::Constructor(_, _, _)),
                "Expected constructor pattern, got {:?}",
                pattern
            );
            assert!(else_branch.is_none(), "Expected no else branch");
        }
        _ => panic!("Expected IfLet expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_wildcard_pattern() {
    let e = parse_expr_ok("if let _ = x { 42 } else { 0 }");
    match e {
        Expr::IfLet(pattern, _, _, _, _) => {
            assert!(
                matches!(pattern, Pattern::Wildcard(_)),
                "Expected Wildcard pattern, got {:?}",
                pattern
            );
        }
        _ => panic!("Expected IfLet expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_variable_pattern() {
    let e = parse_expr_ok("if let y = x { y } else { 0 }");
    match e {
        Expr::IfLet(pattern, _, _, _, _) => {
            assert!(
                matches!(pattern, Pattern::Var(_)),
                "Expected Var pattern, got {:?}",
                pattern
            );
        }
        _ => panic!("Expected IfLet expression, got {:?}", e),
    }
}

#[test]
fn test_if_without_let_unchanged() {
    let e = parse_expr_ok("if x { 1 } else { 2 }");
    assert!(
        matches!(e, Expr::If(_, _, _, _)),
        "Expected regular If, got {:?}",
        e
    );
}

// ─── if let chain tests (ADR 15.5.26d) ──────────────────────────────────

#[test]
fn test_if_let_chain_two_binds() {
    let e = parse_expr_ok("if let Some(x) = a && let Some(y) = b { x } else { 0 }");
    match e {
        Expr::IfLetChain(conditions, _body, else_branch, _) => {
            assert_eq!(conditions.len(), 2, "Expected 2 conditions");
            assert!(matches!(conditions[0], IfLetCondition::Bind(_, _)));
            assert!(matches!(conditions[1], IfLetCondition::Bind(_, _)));
            assert!(else_branch.is_some(), "Expected else branch");
        }
        _ => panic!("Expected IfLetChain expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_chain_bind_and_guard() {
    let e = parse_expr_ok("if let Some(x) = opt && x > 0 { x } else { 0 }");
    match e {
        Expr::IfLetChain(conditions, _body, else_branch, _) => {
            assert_eq!(conditions.len(), 2, "Expected 2 conditions");
            assert!(matches!(conditions[0], IfLetCondition::Bind(_, _)));
            assert!(matches!(conditions[1], IfLetCondition::Guard(_)));
            assert!(else_branch.is_some());
        }
        _ => panic!("Expected IfLetChain expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_chain_no_else() {
    let e = parse_expr_ok("if let Some(x) = a && let Some(y) = b { use_both(x, y) }");
    match e {
        Expr::IfLetChain(conditions, _body, else_branch, _) => {
            assert_eq!(conditions.len(), 2);
            assert!(else_branch.is_none(), "Expected no else branch");
        }
        _ => panic!("Expected IfLetChain expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_chain_three_conditions() {
    let e = parse_expr_ok("if let Some(x) = a && let Some(y) = b && x > y { x } else { 0 }");
    match e {
        Expr::IfLetChain(conditions, _body, _else_branch, _) => {
            assert_eq!(conditions.len(), 3);
            assert!(matches!(conditions[0], IfLetCondition::Bind(_, _)));
            assert!(matches!(conditions[1], IfLetCondition::Bind(_, _)));
            assert!(matches!(conditions[2], IfLetCondition::Guard(_)));
        }
        _ => panic!("Expected IfLetChain expression, got {:?}", e),
    }
}

#[test]
fn test_if_let_single_no_chain() {
    // Single `if let` without `&&` should produce IfLet, not IfLetChain
    let e = parse_expr_ok("if let Some(x) = opt { x } else { 0 }");
    assert!(
        matches!(e, Expr::IfLet(_, _, _, _, _)),
        "Single if let should produce IfLet, got {:?}",
        e
    );
}
