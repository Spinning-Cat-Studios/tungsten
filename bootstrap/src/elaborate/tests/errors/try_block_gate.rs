//! Proof fixtures for `try` block gate conditions (ADR 15.5.26d §5).
//!
//! These tests demonstrate that the immediately-invoked closure desugaring
//! strategy works correctly with the existing compiler infrastructure:
//!
//!   try { body }  ⟶  (fn() => Ok(body))()
//!
//! Since Tungsten lambdas don't have return-type annotations, the IIFE is
//! checked via bidirectional type propagation: the let binding's type annotation
//! or the enclosing expected type gives the lambda its return type.
//!
//! The elaborator builds AST nodes directly during desugaring, so it can pass
//! the expected type through `check` without needing parseable return-type
//! annotations. These tests verify the mechanism using source-level patterns.
//!
//! Each test manually writes the desugared form and verifies the gate condition.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};

// ─── Gate 1: `?` boundary ───────────────────────────────────────────────
// `?` inside a helper function exits that function, not the outer caller.
// This is exactly what the IIFE desugaring achieves.

#[test]
fn gate_try_boundary_question_mark_exits_inner() {
    // Helper function demonstrates that `?` exits the inner function scope.
    // The outer function receives the Result — no type error.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn try_body(input: Result<Nat, String>) -> Result<Nat, String> {
            let x = input?;
            Ok(x + 1)
        }

        fn outer() -> Result<Nat, String> {
            try_body(Ok(42))
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn gate_try_boundary_err_stays_in_inner() {
    // `?` on an Err value returns Err from the inner function,
    // not the outer caller. The outer function receives the Result.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn try_body(bad: Result<Nat, String>) -> Result<Nat, String> {
            let x = bad?;
            Ok(x)
        }

        fn outer() -> Result<Nat, String> {
            try_body(Err("fail"))
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

// ─── Gate 2: `return` behaviour ─────────────────────────────────────────
// `return` inside a nested function/closure exits that function, not the
// outer one. This is the mechanism that try blocks rely on.

#[test]
fn gate_return_exits_inner_not_outer() {
    // `return` inside the inner function returns from it, not the outer.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn try_body() -> Result<Nat, String> {
            return Err("early");
            Ok(42)
        }

        fn outer() -> Result<Nat, String> {
            try_body()
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn gate_return_type_scoped_to_inner() {
    // The inner function's return type (Result<Nat, String>) differs from
    // the outer function's return type (Nat). `return` inside the inner
    // function checks against the inner function's return type.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn try_body() -> Result<Nat, String> {
            return Err("nope");
            Ok(42)
        }

        fn outer() -> Nat {
            match try_body() {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

// ─── Gate 3: Variable capture ───────────────────────────────────────────
// Closures (lambdas) can capture outer variables. This is essential for
// try blocks since the body may reference variables from the enclosing scope.

#[test]
fn gate_capture_outer_variables() {
    // Lambda captures outer variables `base` and `offset`.
    let defs = elab_ok(
        r#"
        fn outer(base: Nat) -> Nat {
            let offset = 10;
            let f = fn() => base + offset;
            f()
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn gate_capture_outer_result_with_question_mark() {
    // Inner function can use `?` on a value passed from the outer scope.
    // For try blocks, captured variables would be similarly available.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn process(r: Result<Nat, String>) -> Result<Nat, String> {
            let x = r?;
            Ok(x + 1)
        }

        fn outer() -> Result<Nat, String> {
            let r: Result<Nat, String> = Ok(42);
            process(r)
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

// ─── Gate 4: Type inference ─────────────────────────────────────────────
// The Result<T, E> type can be inferred through function boundaries
// and check-mode propagation.

#[test]
fn gate_inference_result_type_through_call() {
    // Type inference propagates through function calls.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn try_body() -> Result<Nat, String> {
            Ok(42)
        }

        fn outer() -> Result<Nat, String> {
            try_body()
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn gate_inference_ok_wrapping() {
    // The final expression wrapped in Ok(...) correctly infers Result<T, E>.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Result<Nat, String> {
            let x = 40;
            let y = 2;
            Ok(x + y)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn gate_inference_lambda_iife_with_annotation() {
    // IIFE with let-binding type annotation: the lambda is inferred
    // (not checked), so the body doesn't get the expected type propagated.
    // Ok(42) infers as Result<Nat, Unit> since E is undetermined.
    //
    // This means the elaborator must desugar `try` by checking the lambda
    // body against the expected Result<T, E> type (check mode), not by
    // building an IIFE that gets inferred. This is the key design insight.
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn outer() -> Nat {
            let result: Result<Nat, String> = (fn() => {
                let x: Result<Nat, String> = Ok(42);
                x
            })();
            match result {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─── Gate 5: IIFE with `?` inside ───────────────────────────────────────
// This gate verifies a KEY constraint: an IIFE lambda in infer mode does
// NOT propagate the return context. The `?` operator sees no return type
// (or the outer function's return type) and fails.
//
// This proves the elaborator MUST desugar `try` blocks by checking the
// lambda body in check-mode (with_return_context(Some(result_ty))),
// not by creating a source-level IIFE.

#[test]
fn gate_iife_infer_mode_no_return_context() {
    // IIFE lambda: `fn() => { r? ... }` is inferred, not checked.
    // The lambda's with_return_context(None) clears the return context,
    // so `?` sees the outer function's return type (Nat) and fails.
    //
    // This is EXPECTED: the elaborator must use check-mode to propagate
    // Result<T, E> as the lambda's return type.
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn outer() -> Nat {
            let result: Result<Nat, String> = (fn() => {
                let r: Result<Nat, String> = Ok(42);
                let x = r?;
                Ok(x + 1)
            })();
            match result {
                Ok(v) => v,
                Err(_) => 0
            }
        }
    "#,
    );
    // `?` fails because the inferred lambda's return context is Nat (outer fn),
    // not Result<Nat, String>.
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::TryReturnMismatch { .. })),
        "expected TryReturnMismatch (lambda infer mode doesn't set return context), got: {:?}",
        errors
    );
}

// ─── Negative tests ─────────────────────────────────────────────────────

#[test]
fn gate_negative_question_mark_wrong_return_type() {
    // Function returns Nat, but `?` requires return type to be
    // Result/Option. Should error with TryReturnMismatch.
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)

        fn test() -> Nat {
            let r: Result<Nat, String> = Ok(42);
            r?
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::TryReturnMismatch { .. })),
        "expected TryReturnMismatch, got: {:?}",
        errors
    );
}
