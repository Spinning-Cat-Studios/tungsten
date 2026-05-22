//! `if let` elaboration tests — ADR 14.5.26e.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok, elab_ok_with_warnings};

#[test]
fn test_if_let_basic_with_else() {
    // AC1: `if let Some(x) = Some(42) { x } else { 0 }` elaborates
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            if let Some(x) = opt { x } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_if_let_no_else_unit_body() {
    // AC3: `if let Some(x) = opt { use(x) }` (no else) has type ()
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn id(x: Nat) -> Nat { x }

        fn test() -> () {
            let opt = Some(42);
            if let Some(x) = opt { id(x); () }
        }
    "#,
    );
    assert!(!defs.is_empty());
}

#[test]
fn test_if_let_irrefutable_variable_pattern() {
    // AC5: Irrefutable pattern `if let y = ...` produces W0004
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt: Option<Nat> = Some(42);
            if let y = opt { 0 } else { 1 }
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::IfLetIrrefutable)),
        "expected IfLetIrrefutable warning, got: {:?}",
        warnings
    );
}

#[test]
fn test_if_let_irrefutable_wildcard_pattern() {
    // AC5: Wildcard pattern `if let _ = ...` also produces W0004
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt: Option<Nat> = Some(42);
            if let _ = opt { 0 } else { 1 }
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::IfLetIrrefutable)),
        "expected IfLetIrrefutable warning for wildcard, got: {:?}",
        warnings
    );
}

#[test]
fn test_if_let_refutable_no_warning() {
    // Refutable constructor pattern should NOT emit W0004
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            if let Some(x) = opt { x } else { 0 }
        }
    "#,
    );
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::IfLetIrrefutable)),
        "did not expect IfLetIrrefutable warning for constructor pattern, got: {:?}",
        warnings
    );
}

#[test]
fn test_if_let_type_mismatch_between_branches() {
    // AC6: Type mismatch between branches produces an error
    let errs = elab_err(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            if let Some(x) = opt { x } else { true }
        }
    "#,
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ElabErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch error, got: {:?}",
        errs
    );
}

#[test]
fn test_if_let_no_else_non_unit_body_fails() {
    // AC8: `if let Some(x) = opt { x }` without else fails when body is non-()
    // The desugared match has arms Nat vs (), which are incompatible.
    let errs = elab_err(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            if let Some(x) = opt { x }
        }
    "#,
    );
    assert!(
        !errs.is_empty(),
        "expected an error for non-unit body without else branch"
    );
}

#[test]
fn test_if_let_nested_pattern() {
    // AC4: Nested patterns work inside if let
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None
        type List<T> = Cons(T, List<T>) | Nil

        fn test() -> Nat {
            let opt: Option<List<Nat>> = Some(Cons(1, Nil()));
            if let Some(Cons(x, _)) = opt { x } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_if_let_pattern_var_not_in_else_scope() {
    // AC7: Pattern-bound variables are not in scope in the else branch
    let errs = elab_err(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            if let Some(x) = opt { x } else { x }
        }
    "#,
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ElabErrorKind::UndefinedVariable(_))),
        "expected UndefinedVariable error for `x` in else branch, got: {:?}",
        errs
    );
}

// ─── if let chain tests (ADR 15.5.26d) ──────────────────────────────────

#[test]
fn test_if_let_chain_two_binds() {
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let a = Some(1);
            let b = Some(2);
            if let Some(x) = a && let Some(y) = b { x + y } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_if_let_chain_bind_and_guard() {
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(5);
            if let Some(x) = opt && x > 0 { x } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_if_let_chain_no_else_unit() {
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn id(x: Nat) -> Nat { x }

        fn test() -> () {
            let a = Some(1);
            let b = Some(2);
            if let Some(x) = a && let Some(y) = b { id(x + y); () }
        }
    "#,
    );
    assert!(!defs.is_empty());
}

#[test]
fn test_if_let_chain_earlier_binding_in_scope() {
    // Earlier bindings are in scope for later conditions
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn lookup(x: Nat) -> Option<Nat> { Some(x + 1) }

        fn test() -> Nat {
            let a = Some(10);
            if let Some(x) = a && let Some(y) = lookup(x) { y } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_if_let_chain_vars_not_in_else() {
    // Chain-bound variables are not in scope in the else branch
    let errs = elab_err(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let a = Some(1);
            let b = Some(2);
            if let Some(x) = a && let Some(y) = b { x + y } else { x }
        }
    "#,
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ElabErrorKind::UndefinedVariable(_))),
        "expected UndefinedVariable error for `x` in else branch, got: {:?}",
        errs
    );
}
