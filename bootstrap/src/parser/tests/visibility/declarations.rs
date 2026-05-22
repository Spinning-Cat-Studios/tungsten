//! Visibility tests for declarations (theorems, lemmas, axioms, mod, use).

use crate::ast::*;
use crate::parser::tests::parse_ok;

#[test]
fn test_pub_theorem() {
    let file = parse_ok("pub theorem my_thm : Nat { 0 }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "my_thm");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_pub_crate_theorem() {
    let file = parse_ok("pub(crate) theorem internal_thm : Nat { 0 }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "internal_thm");
            assert_eq!(t.visibility, Visibility::Crate);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_pub_lemma() {
    let file = parse_ok("pub lemma my_lemma : Nat { 0 }");
    match &file.items[0] {
        Item::Lemma(t) => {
            assert_eq!(t.name.name, "my_lemma");
            assert_eq!(t.visibility, Visibility::Public);
        }
        _ => panic!("Expected lemma"),
    }
}

#[test]
fn test_pub_axiom() {
    let file = parse_ok("pub axiom my_axiom : Nat");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "my_axiom");
            assert_eq!(a.visibility, Visibility::Public);
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_pub_crate_axiom() {
    let file = parse_ok("pub(crate) axiom internal_axiom : Nat");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "internal_axiom");
            assert_eq!(a.visibility, Visibility::Crate);
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_pub_crate_mod() {
    let file = parse_ok("pub(crate) mod internal;");
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "internal");
            assert_eq!(m.visibility, Visibility::Crate);
        }
        _ => panic!("Expected mod"),
    }
}

#[test]
fn test_pub_crate_use() {
    let file = parse_ok("pub(crate) use internal::Helper;");
    match &file.items[0] {
        Item::Use(u) => {
            assert_eq!(u.visibility, Visibility::Crate);
        }
        _ => panic!("Expected use"),
    }
}
