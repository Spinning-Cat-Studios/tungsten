//! Tests for let bindings, if expressions, lambda expressions, and type annotations.

use crate::elaborate::tests::{elab_err, elab_ok};
use tungsten_core::{Term, Type};

// ─────────────────────────────────────────────────────────────────────────────
// Let bindings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_let() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let x = 1;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

#[test]
fn test_elaborate_let_with_annotation() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let x: Nat = 42;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

// ─────────────────────────────────────────────────────────────────────────────
// If expressions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_if() {
    let defs = elab_ok(
        r#"
        fn test(b: Bool) -> Nat {
            if b { 1 } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::arrow(Type::Bool, Type::Nat));
}

#[test]
fn test_elaborate_if_type_mismatch() {
    let errors = elab_err(
        r#"
        fn test(b: Bool) -> Nat {
            if b { true } else { 0 }
        }
    "#,
    );
    assert!(!errors.is_empty());
    // Should complain about type mismatch in branches
}

// ─────────────────────────────────────────────────────────────────────────────
// Lambda expressions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_lambda_checked() {
    let defs = elab_ok(
        r#"
        fn make_const() -> Nat -> Nat {
            |x| x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::arrow(Type::Nat, Type::Nat));
}

#[test]
fn test_elaborate_lambda_inferred() {
    let defs = elab_ok(
        r#"
        fn make_id() -> Nat -> Nat {
            fn(x: Nat) => x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Type annotations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_annotation() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            (0 : Nat)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}
