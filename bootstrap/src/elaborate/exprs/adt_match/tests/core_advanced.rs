//! ADT match tests: Result, Tree, and advanced patterns.

use crate::elaborate::tests::{elab_err, elab_ok};

/// Test extracting values from Result.
#[test]
fn test_result_pattern_extract_values() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)
        
        fn unwrap_result(r: Result<String, Nat>) -> String {
            match r {
                Ok(s) => s,
                Err(_) => "error"
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Nested Generic Types
// ============================================================================

/// Test pattern matching on nested generic types like `Option<List<T>>`.
#[test]
fn test_nested_option_list() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn first_of_optional_list(opt: Option<List<Nat>>) -> Nat {
            match opt {
                None() => 0,
                Some(xs) => match xs {
                    Nil() => 0,
                    Cons(n, _) => n
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Type Inference Mode Tests
// ============================================================================

/// Test that type inference (not just checking) works correctly.
///
/// The original bug specifically affected `infer_ctor_arm_type` which is used
/// when inferring the return type, not checking against an expected type.
#[test]
fn test_infer_return_type_from_pattern() {
    // Here we don't annotate the function body - it should be inferred
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn head_or_default(xs: List<String>, default: String) -> String {
            match xs {
                Nil() => default,
                Cons(first, _) => first
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Sum Component Extraction Tests
// ============================================================================

/// Test that get_sum_component correctly extracts types from nested sums.
#[test]
fn test_sum_component_extraction() {
    // A three-constructor ADT: A + (B + C)
    // Index 0 should give A
    // Index 1 should give B
    // Index 2 should give C
    let defs = elab_ok(
        r#"
        pub type Triple =
            | First(Nat)
            | Second(String)
            | Third(Bool)
        
        fn match_triple(t: Triple) -> Nat {
            match t {
                First(n) => n,
                Second(_) => 0,
                Third(_) => 0
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Catch-All Pattern Tests
// ============================================================================

/// Test that catch-all patterns work with generic ADTs.
#[test]
fn test_catch_all_with_generic_adt() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn is_empty(xs: List<String>) -> Bool {
            match xs {
                Nil() => true,
                _ => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test variable catch-all pattern.
#[test]
fn test_variable_catch_all() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn is_some(opt: Option<Nat>) -> Bool {
            match opt {
                None() => false,
                x => true
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Error Cases
// ============================================================================

/// Test that undefined constructors are detected.
#[test]
fn test_undefined_constructor() {
    let errors = elab_err(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn bad_match(opt: Option<String>) -> String {
            match opt {
                None() => "none",
                Foo(s) => s
            }
        }
    "#,
    );

    // Should have an error: Foo is not a constructor of Option
    assert!(!errors.is_empty(), "Expected undefined constructor error");
}

// ============================================================================
// Exhaustiveness Tests
// ============================================================================

/// Test that non-exhaustive matches are detected.
#[test]
fn test_non_exhaustive_match() {
    let errors = elab_err(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn partial_match(xs: List<Nat>) -> Nat {
            match xs {
                Cons(n, _) => n
            }
        }
    "#,
    );

    assert!(!errors.is_empty());
}
