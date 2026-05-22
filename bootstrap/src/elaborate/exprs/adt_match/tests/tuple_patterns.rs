//! Regression tests for tuple patterns inside ADT constructors (ADR 15.5.26f).
//!
//! Covers:
//! - `Ok((a, b))` tuple destructuring in single-field constructors
//! - Wildcards inside tuple-in-constructor patterns
//! - Multi-field constructors with tuple arguments
//! - Nested tuple patterns
//! - Type inference for tuple-bound variables
//! - Depth limit interaction with irrefutable tuple patterns
//! - Tuple arity mismatch detection
//! - Single-element tuple patterns

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};

/// Primary use case: Ok((a, b)) pattern in single-field constructor.
#[test]
fn test_tuple_in_single_field_ctor() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)

        fn unwrap_pair(r: Result<(Nat, String), String>) -> Nat {
            match r {
                Err(_) => 0,
                Ok((n, s)) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unwrap_pair");
}

/// Wildcard inside tuple inside constructor: Ok((_, b)).
#[test]
fn test_wildcard_in_tuple_in_ctor() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)

        fn get_second(r: Result<(Nat, String), String>) -> String {
            match r {
                Err(e) => e,
                Ok((_, s)) => s
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_second");
}

/// Tuple inside multi-field constructor: Cons((x, y), rest).
#[test]
fn test_tuple_in_multi_field_ctor() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)

        fn sum_pairs(xs: List<(Nat, Nat)>) -> Nat {
            match xs {
                Nil() => 0,
                Cons((a, b), rest) => a + b + sum_pairs(rest)
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "sum_pairs");
}

/// Nested tuple inside constructor: Some(((a, b), c)).
#[test]
fn test_nested_tuple_in_ctor() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)

        fn extract(o: Option<((Nat, String), Bool)>) -> Nat {
            match o {
                None() => 0,
                Some(((n, _), _)) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "extract");
}

/// Type inference: tuple-bound variables are correctly typed in arm body.
#[test]
fn test_tuple_in_ctor_type_inference() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)

        fn test_types(r: Result<(Nat, String), String>) -> Nat {
            match r {
                Err(_) => 0,
                Ok((n, s)) => {
                    expect_type(n, "Nat");
                    expect_type(s, "String");
                    n
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test_types");
}

/// Depth limit: constructor nesting with a tuple leaf still hits the depth
/// limit. Tuples are irrefutable and don't increment the depth counter,
/// but the constructor nesting itself is bounded.
#[test]
fn test_tuple_in_ctor_depth_limit() {
    let errors = elab_err(
        r#"
        pub type Option<T> =
            | None
            | Some(T)

        fn deep(o: Option<Option<Option<Option<(Nat, Nat)>>>>) -> Nat {
            match o {
                None() => 0,
                Some(Some(Some(Some((a, b))))) => a
            }
        }
    "#,
    );

    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::PatternTooDeep { .. })),
        "expected PatternTooDeep error, got: {:?}",
        errors
    );
}

/// Deep tuple nesting inside a constructor is allowed because tuples
/// are irrefutable and don't count toward the constructor depth limit.
#[test]
fn test_deep_tuple_nesting_in_ctor_allowed() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)

        fn deep(o: Option<(((Nat, Nat), Nat), Nat)>) -> Nat {
            match o {
                None() => 0,
                Some((((a, b), c), d)) => a
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "deep");
}

/// Tuple arity mismatch: Ok((a, b, c)) when field type is (Nat, String).
#[test]
fn test_tuple_in_ctor_arity_mismatch() {
    let errors = elab_err(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)

        fn bad(r: Result<(Nat, String), String>) -> Nat {
            match r {
                Err(_) => 0,
                Ok((a, b, c)) => a
            }
        }
    "#,
    );

    assert!(!errors.is_empty(), "expected arity mismatch error");
}

/// Single-element tuple inside constructor: Ok((a,)) with 1-tuple field.
#[test]
fn test_single_element_tuple_in_ctor() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)

        fn unwrap_single(o: Option<(Nat,)>) -> Nat {
            match o {
                None() => 0,
                Some((n,)) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unwrap_single");
}
