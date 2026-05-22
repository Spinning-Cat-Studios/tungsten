//! `let`-`else` parsing tests — ADR 13.5.26f.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;
use crate::parser::tests::{parse_ok, unwrap_fn};

/// Parse a function body that contains let-else, returning the block's
/// statements and final expression.
fn parse_fn_body(source: &str) -> (Vec<Stmt>, Option<Box<Expr>>) {
    let wrapped = format!("fn test() {{ {} }}", source);
    let file = parse_ok(&wrapped);
    let body = &unwrap_fn(&file).body;
    let Expr::Block(stmts, final_expr, _) = body else {
        panic!("Expected Block, got {:?}", body);
    };
    (stmts.clone(), final_expr.clone())
}

#[test]
fn test_let_else_basic() {
    let (stmts, final_expr) = parse_fn_body("let Some(x) = opt else return None(); x");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::LetElse(pattern, ty_ann, _value, else_expr, _) => {
            assert!(
                matches!(pattern, Pattern::Constructor(_, _, _)),
                "Expected constructor pattern, got {:?}",
                pattern
            );
            assert!(ty_ann.is_none());
            assert!(
                matches!(else_expr, Expr::Return(_, _)),
                "Expected Return in else, got {:?}",
                else_expr
            );
        }
        _ => panic!("Expected LetElse statement, got {:?}", stmts[0]),
    }
    assert!(final_expr.is_some(), "Expected final expression");
}

#[test]
fn test_let_else_variable_pattern() {
    let (stmts, _) = parse_fn_body("let x = e else return 0; x + 1");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::LetElse(pattern, _, _, _, _) => {
            assert!(
                matches!(pattern, Pattern::Var(_)),
                "Expected Var pattern, got {:?}",
                pattern
            );
        }
        _ => panic!("Expected LetElse statement, got {:?}", stmts[0]),
    }
}

#[test]
fn test_let_without_else_unchanged() {
    let (stmts, _) = parse_fn_body("let x = 42; x");
    assert_eq!(stmts.len(), 1);
    assert!(
        matches!(&stmts[0], Stmt::Let(_, _, _, _)),
        "Expected regular Let, got {:?}",
        stmts[0]
    );
}

#[test]
fn test_let_else_as_expression() {
    // Outside a block, let-else is parsed as an expression via parse_atom
    let e = parse_expr_ok("let Some(x) = opt else return None(); x");
    // parse_expr_ok wraps in a block — the let-else becomes a Stmt,
    // so this returns just the final expr `x`. Test that parsing succeeds.
    assert!(matches!(e, Expr::Path(_)));
}

#[test]
fn test_let_else_with_type_annotation() {
    let (stmts, _) = parse_fn_body("let Some(x): Option<Nat> = opt else return 0; x");
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::LetElse(pattern, ty_ann, _value, _else_expr, _) => {
            assert!(
                matches!(pattern, Pattern::Constructor(_, _, _)),
                "Expected constructor pattern, got {:?}",
                pattern
            );
            assert!(ty_ann.is_some(), "Expected type annotation to be present");
        }
        _ => panic!("Expected LetElse statement, got {:?}", stmts[0]),
    }
}
