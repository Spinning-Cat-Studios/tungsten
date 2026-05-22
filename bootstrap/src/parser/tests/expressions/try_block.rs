//! `try` block parsing tests — ADR 15.5.26d.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_try_block_simple() {
    let e = parse_expr_ok("try { 42 }");
    match e {
        Expr::TryBlock(body, _) => {
            // Body is a Block containing the integer literal
            assert!(
                matches!(*body, Expr::Block(_, _, _)),
                "Expected Block inside TryBlock, got {:?}",
                body
            );
        }
        _ => panic!("Expected TryBlock expression, got {:?}", e),
    }
}

#[test]
fn test_try_block_with_question_mark() {
    let e = parse_expr_ok("try { x? }");
    match e {
        Expr::TryBlock(body, _) => {
            // Body is a Block with a Try expression as the final expr
            if let Expr::Block(_, Some(final_expr), _) = *body {
                assert!(
                    matches!(*final_expr, Expr::Try(_, _)),
                    "Expected Try in final expr, got {:?}",
                    final_expr
                );
            } else {
                panic!("Expected Block with final expr, got {:?}", body);
            }
        }
        _ => panic!("Expected TryBlock expression, got {:?}", e),
    }
}

#[test]
fn test_try_block_with_let_and_question_mark() {
    let e = parse_expr_ok("try { let x = foo()?; Ok(x) }");
    match e {
        Expr::TryBlock(body, _) => {
            assert!(
                matches!(*body, Expr::Block(_, _, _)),
                "Expected Block inside TryBlock, got {:?}",
                body
            );
        }
        _ => panic!("Expected TryBlock expression, got {:?}", e),
    }
}

#[test]
fn test_try_block_in_let_binding() {
    // try block used as value in a let statement
    // parse_expr_ok wraps in fn test() { ... }, so let is a statement.
    // We just check that the let value contains a TryBlock.
    use crate::parser::tests::{parse_ok, Item, Stmt};
    let source = "fn test() { let result = try { 42 }; result }";
    let file = parse_ok(source);
    let Item::Function(f) = &file.items[0] else {
        panic!("expected fn")
    };
    if let Expr::Block(stmts, _, _) = &f.body {
        if let Stmt::Let(_, _, val, _) = &stmts[0] {
            assert!(
                matches!(val, Expr::TryBlock(_, _)),
                "Expected TryBlock as let value, got {:?}",
                val
            );
        } else {
            panic!("Expected let statement, got {:?}", &stmts[0]);
        }
    } else {
        panic!("Expected Block body");
    }
}
