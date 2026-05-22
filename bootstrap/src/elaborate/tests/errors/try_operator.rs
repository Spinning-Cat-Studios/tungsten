//! Try operator (?) error case tests — ADR 13.5.26e.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};

#[test]
fn test_try_result_ok() {
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        fn test(r: Result<Nat, String>) -> Result<Nat, String> {
            let x = r?;
            Ok(x)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test");
}

#[test]
fn test_try_option_ok() {
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        fn test(o: Option<Nat>) -> Option<Nat> {
            let x = o?;
            Some(x)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test");
}

#[test]
fn test_try_on_non_try_type() {
    // No Result/Option constructors in scope → TryOnNonTryType
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            let x: Nat = 42;
            x?
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TryOnNonTryType(_)));
}

#[test]
fn test_try_on_nat_with_result_in_scope() {
    // Result constructors in scope but operand is Nat → TryOnNonTryType
    // (classify gates on operand type structure, not just constructor presence)
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        fn test() -> Result<Nat, String> {
            let x: Nat = 42;
            x?
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TryOnNonTryType(_)));
}

#[test]
fn test_try_return_mismatch() {
    // ? on Result but function returns Nat (not Result) → TryReturnMismatch
    let errors = elab_err(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        fn test(r: Result<Nat, String>) -> Nat {
            r?
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::TryReturnMismatch { .. }
    ));
}
