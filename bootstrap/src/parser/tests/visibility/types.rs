//! Visibility tests for type items (type aliases, type defs, structs, enums).

use crate::ast::*;
use crate::parser::tests::parse_ok;

#[test]
fn test_pub_type_alias() {
    let file = parse_ok("pub type MyNat = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "MyNat");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_pub_crate_type_alias() {
    let file = parse_ok("pub(crate) type InternalNat = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "InternalNat");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_pub_type_def() {
    let file = parse_ok("pub type Option<T> = None | Some(T)");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Option");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_pub_crate_type_def() {
    let file = parse_ok("pub(crate) type Internal = A | B");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Internal");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected type def"),
    }
}

#[test]
fn test_pub_struct() {
    let file = parse_ok("pub struct Point { x: Nat, y: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Point");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected struct"),
    }
}

#[test]
fn test_pub_crate_struct() {
    let file = parse_ok("pub(crate) struct InternalPoint { x: Nat }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "InternalPoint");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected struct"),
    }
}

#[test]
fn test_pub_enum() {
    let file = parse_ok("pub enum Color { Red, Green, Blue }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "Color");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected enum"),
    }
}

#[test]
fn test_pub_crate_enum() {
    let file = parse_ok("pub(crate) enum InternalColor { A, B }");
    match &file.items[0] {
        Item::TypeDef(t) => {
            assert_eq!(t.name.name, "InternalColor");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected enum"),
    }
}
