//! Literal expression parsing tests.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_int_literal() {
    let e = parse_expr_ok("42");
    match e {
        Expr::IntLiteral(v, _) => assert_eq!(v, 42),
        _ => panic!("Expected int literal"),
    }
}

#[test]
fn test_hex_literal() {
    let e = parse_expr_ok("0x2A");
    match e {
        Expr::IntLiteral(v, _) => assert_eq!(v, 42),
        _ => panic!("Expected int literal"),
    }
}

#[test]
fn test_bool_literal() {
    let e = parse_expr_ok("true");
    match e {
        Expr::BoolLiteral(v, _) => assert!(v),
        _ => panic!("Expected bool literal"),
    }
}

#[test]
fn test_string_literal() {
    let e = parse_expr_ok(r#""hello""#);
    match e {
        Expr::StringLiteral(v, _) => assert_eq!(v, "hello"),
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_sorry() {
    let e = parse_expr_ok("sorry");
    match e {
        Expr::Sorry(_) => {}
        _ => panic!("Expected sorry"),
    }
}

#[test]
fn test_natind_parses() {
    let e = parse_expr_ok("natind(|k: Nat| Nat, 0, fn(k: Nat, ih: Nat) => ih, 3)");
    match e {
        Expr::NatInd(Motive::Lambda(_, _, _), _, _, _, _) => {}
        _ => panic!("Expected NatInd with lambda motive, got {:?}", e),
    }
}

#[test]
fn test_natrec_parses() {
    let e = parse_expr_ok("natrec(Nat, 0, fn(k: Nat, acc: Nat) => acc, 3)");
    match e {
        Expr::NatRec(_, _, _, _, _) => {}
        _ => panic!("Expected NatRec, got {:?}", e),
    }
}
