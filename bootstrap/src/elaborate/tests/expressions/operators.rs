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

// ─────────────────────────────────────────────────────────────────────────────
// `__compare`'s comparability gate (ADR 29.6.26f P3, ADR 1.8.26b)
// ─────────────────────────────────────────────────────────────────────────────

/// Elaboration rejects a `__compare` whose operand type this build cannot
/// synthesize a comparator for. Both polarities are asserted, because the check
/// is a negation: a gate that accepted everything and one that rejected
/// everything each pass a one-sided test, and only one of them is the fix.
#[test]
fn compare_rejects_an_uncomparable_operand_and_accepts_a_comparable_one() {
    use crate::elaborate::tests::elab_err;

    let errors = elab_err(
        r#"
        fn cmp(f: Nat -> Nat, g: Nat -> Nat) -> Nat {
            __compare(f, g)
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("no comparator available")),
        "a function-typed operand must be refused: {errors:?}"
    );

    // The twin: an ordinary comparable operand clears the gate, so it is
    // selective rather than blanket. Asserted as the ABSENCE of this specific
    // error — the snippet still has a type error (`__compare` yields
    // `CompareResult`, which this bare source does not declare), and that is a
    // different failure from the one under test.
    let comparable = elab_err(
        r#"
        fn cmp(a: Nat, b: Nat) -> Nat {
            __compare(a, b)
        }
        "#,
    );
    assert!(
        !comparable
            .iter()
            .any(|e| e.to_string().contains("no comparator available")),
        "a Nat operand must clear the comparability gate: {comparable:?}"
    );
}
