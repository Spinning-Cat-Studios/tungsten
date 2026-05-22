//! Tests for binary operations and string operations.

use crate::elaborate::tests::elab_ok;
use tungsten_core::Type;

// ─────────────────────────────────────────────────────────────────────────────
// Binary operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_add() {
    let defs = elab_ok(
        r#"
        fn add(x: Nat, y: Nat) -> Nat {
            x + y
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Type should be Nat → Nat → Nat
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::Nat, Type::arrow(Type::Nat, Type::Nat))
    );
}

#[test]
fn test_elaborate_and() {
    let defs = elab_ok(
        r#"
        fn both(a: Bool, b: Bool) -> Bool {
            a && b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_elaborate_or() {
    let defs = elab_ok(
        r#"
        fn either(a: Bool, b: Bool) -> Bool {
            a || b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_elaborate_not() {
    let defs = elab_ok(
        r#"
        fn negate(b: Bool) -> Bool {
            !b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Strings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_string_concat() {
    let defs = elab_ok(
        r#"
        fn greet(name: String) -> String {
            "Hello, " ++ name
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Type should be String → String
    assert_eq!(defs[0].ty, Type::arrow(Type::String, Type::String));
}

#[test]
fn test_elaborate_string_concat_chained() {
    let defs = elab_ok(
        r#"
        fn wrap(s: String) -> String {
            "[" ++ s ++ "]"
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}
