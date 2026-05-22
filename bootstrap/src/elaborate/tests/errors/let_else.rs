//! `let`-`else` elaboration tests — ADR 13.5.26f.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok, elab_ok_with_warnings};

#[test]
fn test_let_else_irrefutable_variable_pattern() {
    // AC #6: `let x = e else ...` should emit W0003 (irrefutable pattern)
    // Uses an ADT value since match on non-sum types is unsupported.
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt: Option<Nat> = Some(42);
            let x = opt else return 0;
            0
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::LetElseIrrefutable)),
        "expected LetElseIrrefutable warning, got: {:?}",
        warnings
    );
}

#[test]
fn test_let_else_irrefutable_wildcard_pattern() {
    // Wildcard pattern `_` is also irrefutable → W0003
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt: Option<Nat> = Some(42);
            let _ = opt else return 0;
            0
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::LetElseIrrefutable)),
        "expected LetElseIrrefutable warning for wildcard, got: {:?}",
        warnings
    );
}

#[test]
fn test_let_else_refutable_no_warning() {
    // Refutable constructor pattern should NOT emit W0003
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            let Some(x) = opt else return 0;
            x
        }
    "#,
    );
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::LetElseIrrefutable)),
        "did not expect LetElseIrrefutable warning for constructor pattern, got: {:?}",
        warnings
    );
}

#[test]
fn test_let_else_basic_elab() {
    // let-else with Some successfully elaborates
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            let Some(x) = opt else return 0;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_let_else_with_type_annotation() {
    // let-else with explicit type annotation elaborates
    let defs = elab_ok(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            let Some(x): Option<Nat> = opt else return 0;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_let_else_non_diverging_error() {
    // Non-diverging else branch should cause a type error (E0010 via match desugaring)
    let errs = elab_err(
        r#"
        type Option<T> = Some(T) | None

        fn test() -> Nat {
            let opt = Some(42);
            let Some(x) = opt else "not a number";
            x
        }
    "#,
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ElabErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch from non-diverging else, got: {:?}",
        errs.iter().map(|e| &e.kind).collect::<Vec<_>>()
    );
}
