//! Tests for expect_error compile-time error assertion (ADR 12.5.26c).

use crate::elaborate::tests::{elab_err, elab_ok, elab_with_mode};
use crate::elaborate::ElabMode;

// ─────────────────────────────────────────────────────────────────────────────
// Correct error code matched → pass
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_undefined_variable() {
    // E0001 = UndefinedVariable in L1
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0001");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_error should pass for undefined var → E0001, got: {:?}",
        result.err()
    );
}

#[test]
fn test_expect_error_type_mismatch() {
    // E0010 = TypeMismatch in L1
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(if true { 1 } else { "hello" }, "E0010");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_error should pass for type mismatch → E0010, got: {:?}",
        result.err()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Expression succeeds → fail
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_expression_succeeds() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(42, "E0001");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_err(),
        "expect_error should fail when expression succeeds"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("elaborated successfully"),
        "should report success failure, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Wrong error code → fail
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_wrong_code() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0010");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_err(),
        "expect_error should fail for wrong error code"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("expected error E0010"),
        "should mention expected code, got: {msg}"
    );
    assert!(
        msg.contains("E0001"),
        "should mention actual code, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// State not poisoned — bindings available after expect_error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_state_not_poisoned() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            let x: Nat = 42;
            expect_error(undefined_var, "E0001");
            expect_type(x, "Nat");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "bindings before expect_error should still be available, got: {:?}",
        result.err()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// No type variable leakage
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_no_type_var_leakage() {
    // The failed sub-expression should not leak bindings into the outer scope
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0001");
            expect_type(42, "Nat");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_error should not leak state, got: {:?}",
        result.err()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Mode gating
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_compile_mode_rejected() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0001");
            ()
        }
    "#,
        ElabMode::Compile,
    );
    assert!(
        result.is_err(),
        "expect_error should be rejected in compile mode"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("not allowed in compile/run mode"),
        "should mention compile/run mode, got: {msg}"
    );
}

#[test]
fn test_expect_error_check_mode_active() {
    // Check mode should also validate the assertion (same as test mode)
    let defs = elab_ok(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0001");
            ()
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Malformed calls
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_wrong_arity_zero() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_error();
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
fn test_expect_error_wrong_arity_one() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_error(42);
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
fn test_expect_error_non_string_second_arg() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_error(42, 1);
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

// ─────────────────────────────────────────────────────────────────────────────
// Multiple errors — first/primary reported on mismatch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_reports_primary_error_on_mismatch() {
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(undefined_var, "E0010");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(result.is_err());
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    // Primary error should be E0001 (UndefinedVariable), reported in the message
    assert!(
        msg.contains("E0001"),
        "should report actual primary error code, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Arity = 3 (too many arguments)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_wrong_arity_three() {
    let errors = elab_err(
        r#"
        fn test() -> Unit {
            expect_error(42, "E0001", "extra");
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

// ─────────────────────────────────────────────────────────────────────────────
// Name counter restoration — fresh names after expect_error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_name_counter_restored() {
    // If the failed sub-expression generates fresh names (via lambda),
    // those should not affect the outer scope's name counter.
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(if true { 1 } else { "hello" }, "E0010");
            let f = fn(y: Nat) => y;
            expect_type(f, "Nat -> Nat");
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "name counter should be restored after expect_error, got: {:?}",
        result.err()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Nested expect_error — expect_error inside expect_error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_expect_error_nested() {
    // An expect_error whose sub-expression contains another expect_error
    // that succeeds — the outer expect_error should see the overall success
    // and report "expected error but elaborated successfully".
    let result = elab_with_mode(
        r#"
        fn test() -> Unit {
            expect_error(
                expect_error(undefined_var, "E0001"),
                "E0010"
            );
            ()
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_err(),
        "nested expect_error where inner succeeds should make outer fail"
    );
    let errors = result.unwrap_err();
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("elaborated successfully"),
        "should report success failure for outer expect_error, got: {msg}"
    );
}
