//! Unit tests for ADT match elaboration.
//!
//! These tests focus on verifying the fix for the generic pattern inference bug
//! (ADR 30.1.26). The key issue was that `infer_ctor_arm_type` only performed
//! Phase 2 substitution (μ-refs) without Phase 1 (type params).
//!
//! ## Test Categories
//!
//! 1. **Type Unfolding** - Tests for μ-type substitution during unfolding
//! 2. **Two-Phase Substitution** - Tests that both phases are applied correctly
//! 3. **Generic ADT Patterns** - End-to-end tests with `List<T>`, `Option<T>`, etc.
//! 4. **Sum Component Extraction** - Tests for navigating nested sum types
//!
//! ## The Bug Pattern
//!
//! Before the fix, matching on `List<String>` with `Cons(first, rest)`:
//! - `first` got type `T` (missing Phase 1) or `α_List` (leaked μ-var)
//! - `rest` got type with `α_List` instead of `List<String>`
//!
//! After the fix:
//! - `first` correctly gets type `String`
//! - `rest` correctly gets type `List<String>` (the full μ-type)

use crate::elaborate::tests::{elab_err, elab_ok};

// ============================================================================
// Generic List Pattern Matching (ADR 30.1.26 Core Bug)
// ============================================================================

/// Test that pattern matching on `List<String>` correctly infers `String` for head.
///
/// This is the canonical test case for ADR 30.1.26. The bug caused `first` to have
/// type containing `α_List` instead of `String`.
#[test]
fn test_generic_list_pattern_head_type() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn get_head(xs: List<String>) -> String {
            match xs {
                Nil() => "empty",
                Cons(first, _) => first
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_head");
    // If the bug exists, this would fail with a type mismatch
    // (first would have type with α_List instead of String)
}

/// Test that pattern matching on `List<Nat>` correctly infers `Nat` for head.
#[test]
fn test_generic_list_pattern_head_nat() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn get_first(xs: List<Nat>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(n, _) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_first");
}

/// Test that the tail of a list has the correct recursive type.
///
/// The tail `rest` should have type `List<String>`, not `α_List` or similar.
#[test]
fn test_generic_list_pattern_tail_type() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn get_tail(xs: List<String>) -> List<String> {
            match xs {
                Nil() => Nil(),
                Cons(_, rest) => rest
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test recursive function over generic list.
///
/// This is a more complex test that exercises the fix in a realistic scenario.
#[test]
fn test_generic_list_recursive_function() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn length(xs: List<String>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(_, rest) => 1 + length(rest)
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Option Type Pattern Matching
// ============================================================================

/// Test pattern matching on `Option<T>`.
#[test]
fn test_option_pattern_some_value() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn unwrap_or(opt: Option<Nat>, default: Nat) -> Nat {
            match opt {
                None() => default,
                Some(x) => x
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test Option with String type parameter.
#[test]
fn test_option_pattern_string() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn get_or_empty(opt: Option<String>) -> String {
            match opt {
                None() => "",
                Some(s) => s
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Result Type Pattern Matching (Multiple Type Parameters)
// ============================================================================

/// Test pattern matching on `Result<T, E>` with multiple type parameters.
///
/// This tests that Phase 1 substitution handles multiple type parameters correctly.
#[test]
fn test_result_pattern_multiple_params() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)
        
        fn is_ok(r: Result<String, Nat>) -> Bool {
            match r {
                Ok(_) => true,
                Err(_) => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}
