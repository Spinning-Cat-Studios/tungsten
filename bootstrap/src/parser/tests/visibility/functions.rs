//! Visibility tests for function items.

use crate::ast::*;
use crate::parser::tests::parse_ok;

#[test]
fn test_pub_function() {
    let file = parse_ok("pub fn public_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "public_fn");
            assert_eq!(f.visibility, Visibility::Public);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_pub_crate_function() {
    let file = parse_ok("pub(crate) fn internal_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "internal_fn");
            assert_eq!(f.visibility, Visibility::Crate);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_private_function() {
    let file = parse_ok("fn private_fn() { 42 }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "private_fn");
            assert_eq!(f.visibility, Visibility::Private);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_pub_extern_fn() {
    let file = parse_ok("pub extern fn print(s: String) -> Unit");
    match &file.items[0] {
        Item::ExternFn(e) => {
            assert_eq!(e.name.name, "print");
            assert_eq!(e.visibility, Visibility::Public);
        }
        _ => panic!("Expected extern fn"),
    }
}

#[test]
fn test_pub_crate_extern_fn() {
    let file = parse_ok("pub(crate) extern fn internal_print(s: String) -> Unit");
    match &file.items[0] {
        Item::ExternFn(e) => {
            assert_eq!(e.name.name, "internal_print");
            assert_eq!(e.visibility, Visibility::Crate);
        }
        _ => panic!("Expected extern fn"),
    }
}
