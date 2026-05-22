//! Try operator (`?`) postfix parsing tests — ADR 13.5.26e AC #11, #12.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_try_simple() {
    // `x?` parses as Try(Var("x"))
    let e = parse_expr_ok("x?");
    match e {
        Expr::Try(inner, _) => {
            assert!(matches!(*inner, Expr::Path(_)));
        }
        _ => panic!("Expected Try expression, got {:?}", e),
    }
}

#[test]
fn test_try_after_call() {
    // AC #11: `foo()? ` parses as `(foo())?`, not malformed
    let e = parse_expr_ok("foo()?");
    match e {
        Expr::Try(inner, _) => {
            assert!(
                matches!(*inner, Expr::App(_, _, _)),
                "Expected App inside Try, got {:?}",
                inner
            );
        }
        _ => panic!("Expected Try expression, got {:?}", e),
    }
}

#[test]
fn test_try_then_field_access() {
    // AC #12: `x?.y` parses as `(x?).y`
    let e = parse_expr_ok("x?.y");
    match e {
        Expr::Field(inner, field_name, _) => {
            assert_eq!(field_name.name, "y");
            assert!(
                matches!(*inner, Expr::Try(_, _)),
                "Expected Try inside Field, got {:?}",
                inner
            );
        }
        _ => panic!("Expected Field expression, got {:?}", e),
    }
}

#[test]
fn test_try_chain() {
    // `a()?.b()?` parses as `((a()?.b)())?`
    let e = parse_expr_ok("a()?.b()?");
    let Expr::Try(inner, _) = e else {
        panic!("Expected outer Try, got {:?}", e);
    };
    // inner should be a call: (a()?.b)(...)
    let Expr::App(callee, _, _) = *inner else {
        panic!("Expected App inside outer Try");
    };
    // callee should be field access: a()?.b
    let Expr::Field(recv, name, _) = *callee else {
        panic!("Expected Field inside App");
    };
    assert_eq!(name.name, "b");
    // recv should be Try(App(a, []))
    assert!(
        matches!(*recv, Expr::Try(_, _)),
        "Expected Try inside Field, got {:?}",
        recv
    );
}
