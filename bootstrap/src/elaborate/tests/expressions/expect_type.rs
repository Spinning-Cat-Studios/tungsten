//! Tests for expect_type compile-time type assertion (ADR 4.5.26g).

use crate::elaborate::tests::{elab_err, elab_ok, elab_with_mode};
use crate::elaborate::ElabMode;

// ─────────────────────────────────────────────────────────────────────────────
// Check mode: validates args, skips comparison, returns Unit
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_type_check_mode_succeeds() {
    // In check mode, expect_type validates args but skips comparison.
    // Even a "wrong" expected string should succeed.
    let defs = elab_ok(
        r#"
        fn test() -> Unit {
            expect_type(42, "WrongType");
            ()
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_expect_type_check_mode_elaborates_arg() {
    // Check mode still elaborates the first argument (type errors propagate).
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_type(undefined_var, "Nat");
            ()
        }
    "#,
    );
    assert!(!errors.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Test mode: validates args AND compares types
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_type_test_mode_nat() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_type(42, "Nat");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_type(42, \"Nat\") should pass in test mode"
    );
}

#[test]
fn test_expect_type_test_mode_string() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_type("hello", "String");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_type(\"hello\", \"String\") should pass in test mode"
    );
}

#[test]
fn test_expect_type_test_mode_bool() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_type(true, "Bool");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_type(true, \"Bool\") should pass in test mode"
    );
}

#[test]
fn test_expect_type_test_mode_mismatch() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_type(42, "String");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_err(),
        "expect_type(42, \"String\") should fail in test mode"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("type assertion failed"),
        "error should mention type assertion failed, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Compile mode: rejects expect_type entirely
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_type_compile_mode_rejected() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_type(42, "Nat");
            ()
        }
    "#,
        ElabMode::Compile,
    );
    assert!(
        result.is_err(),
        "expect_type should be rejected in compile mode"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("not allowed in compile/run mode"),
        "error should mention compile/run mode, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation errors (mode-independent)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_type_wrong_arity_zero() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_type();
            ()
        }
    "#,
    );
    assert!(!errors.is_empty());
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("expected 2 arguments"),
        "should report arity error, got: {msg}"
    );
}

#[test]
fn test_expect_type_wrong_arity_one() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_type(42);
            ()
        }
    "#,
    );
    assert!(!errors.is_empty());
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("expected 2 arguments"),
        "should report arity error, got: {msg}"
    );
}

#[test]
fn test_expect_type_non_string_second_arg() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_type(42, 1);
            ()
        }
    "#,
    );
    assert!(!errors.is_empty());
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("string literal"),
        "should report string literal error, got: {msg}"
    );
}
