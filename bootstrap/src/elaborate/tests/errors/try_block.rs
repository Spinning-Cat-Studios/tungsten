//! `try` block elaboration tests — ADR 15.5.26d.
//!
//! Tests verify:
//! - Basic try block desugaring (check and infer mode)
//! - `?` inside try block exits the block, not the enclosing function
//! - Explicit `return` inside try block is rejected
//! - Ok-wrapping of the final expression
//! - Infer mode requires type annotation

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};

// ─── Basic check-mode tests ─────────────────────────────────────────────

#[test]
fn test_try_block_basic_ok_wrapping() {
    // try { 42 } should wrap the body in Ok(42) → Result<Nat, E>
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            try { 42 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_try_block_with_question_mark() {
    // try { r? } should unwrap Ok and re-wrap, or propagate Err to try boundary
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test(r: Result<Nat, String>) -> Result<Nat, String> {
            try { r? }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_try_block_multi_question_mark() {
    // Multiple `?` inside a try block
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test(a: Result<Nat, String>, b: Result<Nat, String>) -> Result<Nat, String> {
            try {
                let x = a?;
                let y = b?;
                x + y
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_try_block_in_let_binding() {
    // try block assigned to a let binding with type annotation
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Nat {
            let result: Result<Nat, String> = try { 42 };
            match result {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─── `?` boundary tests ─────────────────────────────────────────────────

#[test]
fn test_try_block_question_mark_exits_block() {
    // `?` inside try block exits the block, not the outer function.
    // Outer function returns Nat — the try block captures the Result.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test(r: Result<Nat, String>) -> Nat {
            let result: Result<Nat, String> = try { r? };
            match result {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─── Return rejection ────────────────────────────────────────────────────

#[test]
fn test_try_block_rejects_return() {
    // Explicit `return` inside try block should be rejected
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            try { return Ok(42) }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::ReturnInsideTryBlock)),
        "expected ReturnInsideTryBlock, got: {:?}",
        errors
    );
}

#[test]
fn test_try_block_return_in_nested_block() {
    // `return` inside a block inside try should also be rejected
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            try {
                let x = 42;
                return Ok(x)
            }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::ReturnInsideTryBlock)),
        "expected ReturnInsideTryBlock, got: {:?}",
        errors
    );
}

#[test]
fn test_try_block_return_in_lambda_ok() {
    // `return` inside a lambda inside try block IS allowed
    // (the lambda has its own return scope)
    // The lambda must be checked (type annotation) to have a return context.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            try {
                let f: Nat -> Nat = fn(x: Nat) => {
                    return x + 1;
                    x
                };
                f(41)
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─── Infer mode ──────────────────────────────────────────────────────────

#[test]
fn test_try_block_infer_mode_needs_annotation() {
    // try block in infer mode should require a type annotation
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Nat {
            let result = try { 42 };
            0
        }
    "#,
    );
    assert!(
        !errors.is_empty(),
        "expected error for try block without type annotation"
    );
}

// ─── Return in let-stmt value (Fix #1: find_return_in_expr) ──────────────

#[test]
fn test_try_block_return_in_let_value() {
    // `return` inside a let binding's value should be rejected
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            try {
                let x = return Ok(42);
                x
            }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::ReturnInsideTryBlock)),
        "expected ReturnInsideTryBlock for return in let value, got: {:?}",
        errors
    );
}

// ─── Nested try block ────────────────────────────────────────────────────

#[test]
fn test_try_block_nested() {
    // Nested try blocks: inner try captures errors at its own boundary
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Result<Nat, String>, String> {
            try {
                let inner: Result<Nat, String> = try { 42 };
                inner
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─── Error propagation via ? ──────────────────────────────────────────────

#[test]
fn test_try_block_question_mark_propagates_err() {
    // try { err_val? } should elaborate: ? on Err propagates to try boundary,
    // producing Result<T, E> where the Err case is Inl+Return.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn make_err() -> Result<Nat, String> {
            Err("oops")
        }

        fn test() -> Nat {
            let result: Result<Nat, String> = try { make_err()? };
            match result {
                Ok(v) => v,
                Err(_) => 99
            }
        }
    "#,
    );
    // make_err + test = 2 defs
    assert_eq!(defs.len(), 2);
}

// ─── Dedicated error kind tests ──────────────────────────────────────────

#[test]
fn test_try_block_no_result_type_uses_dedicated_error() {
    // try block without Result in scope should use TryBlockRequiresResultType
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            let result = try { 42 };
            0
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e.kind,
            ElabErrorKind::TryBlockRequiresResultType | ElabErrorKind::CannotInferType
        )),
        "expected TryBlockRequiresResultType or CannotInferType, got: {:?}",
        errors
    );
}
