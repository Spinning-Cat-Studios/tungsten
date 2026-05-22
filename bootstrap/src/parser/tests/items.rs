//! Tests for item parsing: functions, types, theorems, axioms, mods.

use super::{parse_ok, unwrap_sum_variants, unwrap_type_def};
use crate::ast::*;

#[test]
fn test_empty_file() {
    let file = parse_ok("");
    assert!(file.items.is_empty());
}

#[test]
fn test_simple_function() {
    let file = parse_ok("fn foo() { 42 }");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "foo");
            assert!(f.params.is_empty());
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_function_with_params() {
    let file = parse_ok("fn add(x: Nat, y: Nat) -> Nat { x + y }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.return_type.is_some());
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_function_with_type_params() {
    let file = parse_ok("fn id<T>(x: T) -> T { x }");
    match &file.items[0] {
        Item::Function(f) => {
            assert_eq!(f.name.name, "id");
            assert_eq!(f.type_params.len(), 1);
            assert_eq!(f.type_params[0].name.name, "T");
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_type_alias() {
    let file = parse_ok("type MyInt = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "MyInt");
        }
        _ => panic!("Expected type alias"),
    }
}

#[test]
fn test_type_def() {
    let file = parse_ok("type Option<T> = None | Some(T)");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Option");
    assert_eq!(t.type_params.len(), 1);
    assert_eq!(unwrap_sum_variants(&file).len(), 2);
}

#[test]
fn test_struct() {
    let file = parse_ok("struct Point { x: Nat, y: Nat }");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Point");
    let variants = unwrap_sum_variants(&file);
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].fields.len(), 2);
}

#[test]
fn test_enum() {
    let file = parse_ok("enum Color { Red, Green, Blue }");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Color");
    assert_eq!(unwrap_sum_variants(&file).len(), 3);
}

#[test]
fn test_theorem() {
    let file = parse_ok("theorem trivial: Bool { true }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "trivial");
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_theorem_with_arrow() {
    let file = parse_ok("theorem trivial() -> Bool { true }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "trivial");
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_theorem_with_params() {
    let file = parse_ok("theorem reflexive(x: Nat) -> Eq<Nat, x, x> { sorry }");
    match &file.items[0] {
        Item::Theorem(t) => {
            assert_eq!(t.name.name, "reflexive");
            assert_eq!(t.params.len(), 1);
        }
        _ => panic!("Expected theorem"),
    }
}

#[test]
fn test_lemma() {
    let file = parse_ok("lemma helper: Prop { sorry }");
    match &file.items[0] {
        Item::Lemma(t) => {
            assert_eq!(t.name.name, "helper");
        }
        _ => panic!("Expected lemma"),
    }
}

#[test]
fn test_lemma_with_arrow() {
    let file = parse_ok("lemma helper() -> Prop { sorry }");
    match &file.items[0] {
        Item::Lemma(t) => {
            assert_eq!(t.name.name, "helper");
        }
        _ => panic!("Expected lemma"),
    }
}

#[test]
fn test_axiom() {
    let file = parse_ok("axiom excluded_middle(P: Prop): P");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "excluded_middle");
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_axiom_with_arrow() {
    let file = parse_ok("axiom excluded_middle(P: Prop) -> P");
    match &file.items[0] {
        Item::Axiom(a) => {
            assert_eq!(a.name.name, "excluded_middle");
        }
        _ => panic!("Expected axiom"),
    }
}

#[test]
fn test_mod_declaration() {
    let file = parse_ok("mod foo;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "foo");
            assert_eq!(m.visibility, Visibility::Private);
        }
        _ => panic!("Expected mod declaration"),
    }
}

#[test]
fn test_pub_mod_declaration() {
    let file = parse_ok("pub mod api;");
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "api");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected pub mod declaration"),
    }
}

#[test]
fn test_mod_visibility_mixed() {
    let file = parse_ok("pub mod api;\nmod internal;\npub mod types;");
    assert_eq!(file.items.len(), 3);

    match &file.items[0] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "api");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected mod declaration"),
    }

    match &file.items[1] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "internal");
            assert_eq!(m.visibility, Visibility::Private);
        }
        _ => panic!("Expected mod declaration"),
    }

    match &file.items[2] {
        Item::Mod(m) => {
            assert_eq!(m.name.name, "types");
            assert_eq!(m.visibility, Visibility::Public);
        }
        _ => panic!("Expected mod declaration"),
    }
}

#[test]
fn test_mod_multiple() {
    let file = parse_ok("mod foo;\nmod bar;\nfn main() { 1 }");
    assert_eq!(file.items.len(), 3);
    assert!(
        matches!(&file.items[0], Item::Mod(m) if m.name.name == "foo" && m.visibility == Visibility::Private)
    );
    assert!(
        matches!(&file.items[1], Item::Mod(m) if m.name.name == "bar" && m.visibility == Visibility::Private)
    );
    assert!(matches!(&file.items[2], Item::Function(_)));
}

// ─── Type alias disambiguation tests (ADR 15.5.26g §2.1) ────────────────────

#[test]
fn test_type_alias_generic_application() {
    // type Alias = Result<Nat, String> — identifier followed by <
    let file = parse_ok("type Alias = Result<Nat, String>");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "Alias");
            assert!(t.type_params.is_empty());
        }
        other => panic!(
            "Expected TypeAlias, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn test_type_alias_parameterised() {
    // type ParseResult<T> = Result<(T, Nat), String>
    let file = parse_ok("type ParseResult<T> = Result<(T, Nat), String>");
    match &file.items[0] {
        Item::TypeAlias(t) => {
            assert_eq!(t.name.name, "ParseResult");
            assert_eq!(t.type_params.len(), 1);
            assert_eq!(t.type_params[0].name.name, "T");
        }
        other => panic!(
            "Expected TypeAlias, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn test_type_alias_bare_ident() {
    // type MyNat = Nat — bare identifier without < or (
    let file = parse_ok("type MyNat = Nat");
    match &file.items[0] {
        Item::TypeAlias(t) => assert_eq!(t.name.name, "MyNat"),
        other => panic!(
            "Expected TypeAlias, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn test_adt_constructor_with_parens() {
    // type Foo = Bar(Nat) — constructor with parens → ADT
    let file = parse_ok("type Foo = Bar(Nat)");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Foo");
    let variants = unwrap_sum_variants(&file);
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].name.name, "Bar");
}

#[test]
fn test_adt_nullary_constructor_with_parens() {
    // type Foo = Bar() — nullary constructor with explicit parens → ADT (not alias)
    let file = parse_ok("type Foo = Bar()");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Foo");
    let variants = unwrap_sum_variants(&file);
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].name.name, "Bar");
}

#[test]
fn test_adt_pipe_led() {
    // type Foo = | Bar | Baz — pipe-led variants → ADT
    let file = parse_ok("type Foo = | Bar | Baz");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Foo");
    assert_eq!(unwrap_sum_variants(&file).len(), 2);
}

#[test]
fn test_adt_multi_variant_no_pipe() {
    // type Foo = Bar(Nat) | Baz(String) — ident followed by ( → ADT
    let file = parse_ok("type Foo = Bar(Nat) | Baz(String)");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Foo");
    assert_eq!(unwrap_sum_variants(&file).len(), 2);
}
