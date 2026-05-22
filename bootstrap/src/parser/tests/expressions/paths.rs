//! Path expression parsing tests.

use crate::ast::*;
use crate::parser::tests::parse_expr_ok;

#[test]
fn test_variable() {
    let e = parse_expr_ok("x");
    match e {
        Expr::Path(path) => {
            assert!(path.is_simple());
            assert_eq!(path.item_name().name, "x");
        }
        _ => panic!("Expected path"),
    }
}

#[test]
fn test_qualified_path() {
    let e = parse_expr_ok("foo::bar::baz");
    match e {
        Expr::Path(path) => {
            assert!(!path.is_simple());
            assert_eq!(path.segments.len(), 3);
            assert_eq!(path.segments[0].name, "foo");
            assert_eq!(path.segments[1].name, "bar");
            assert_eq!(path.segments[2].name, "baz");
            assert_eq!(path.item_name().name, "baz");
        }
        _ => panic!("Expected path"),
    }
}

#[test]
fn test_qualified_path_two_segments() {
    let e = parse_expr_ok("module::item");
    match e {
        Expr::Path(path) => {
            assert!(!path.is_simple());
            assert_eq!(path.segments.len(), 2);
            assert_eq!(path.segments[0].name, "module");
            assert_eq!(path.segments[1].name, "item");
        }
        _ => panic!("Expected path"),
    }
}
