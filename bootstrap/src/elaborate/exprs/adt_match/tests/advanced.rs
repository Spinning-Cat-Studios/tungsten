//! Advanced ADT match tests: multi-arm, multi-field, nested, mixed patterns.
//!
//! Covers:
//! - Multiple arms per constructor (HashMap overwrite bug fix)
//! - Multi-field constructor (product type payload)
//! - Single-field constructor with nested sum (nested match required)
//! - Mixed patterns (variable + constructor in same match)
//!
//! Mu-type unwrapping and sum navigation tests are in advanced_sums.rs.

use crate::elaborate::tests::{elab_err, elab_ok};

// ============================================================================
// Multiple Arms Per Constructor Tests (HashMap Overwrite Bug Fix)
// ============================================================================
//
// These tests verify the fix for the bug where HashMap<usize, &MatchArm>
// overwrote arms when multiple patterns shared the same outer constructor.
// The fix changed to HashMap<usize, Vec<&MatchArm>> and builds nested matches.

/// Test multiple Option::Some patterns with different nested constructors.
///
/// Before the fix, only the last Some arm would be kept in the HashMap,
/// causing incorrect code generation. This pattern requires nested matching
/// on the Some payload.
#[test]
fn test_multiple_some_arms_nested_option() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn match_nested_option(opt: Option<Option<Nat>>) -> Nat {
            match opt {
                None() => 0,
                Some(None()) => 1,
                Some(Some(n)) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "match_nested_option");
}

/// Test multiple arms with same outer constructor on Result type.
///
/// Both Ok arms share outer constructor, requiring nested match on payload.
#[test]
fn test_multiple_ok_arms_nested_result() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)
        
        fn match_nested_result(r: Result<Option<String>, Nat>) -> String {
            match r {
                Ok(None()) => "ok-none",
                Ok(Some(s)) => s,
                Err(_) => "error"
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "match_nested_result");
}

/// Test three-level nesting with multiple arms at intermediate level.
///
/// This tests deeply nested constructor patterns.
#[test]
fn test_triple_nested_option() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn match_triple_nested(opt: Option<Option<Option<Nat>>>) -> Nat {
            match opt {
                None() => 0,
                Some(None()) => 1,
                Some(Some(None())) => 2,
                Some(Some(Some(n))) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "match_triple_nested");
}

// ============================================================================
// Multi-Field Constructor Tests (Product Type Payload)
// ============================================================================
//
// These tests verify that constructors with multiple fields (product type
// payloads like Cons(T, List<T>)) correctly fall back to the first arm
// instead of attempting to build nested matches on the product type.

/// Test that nested constructor patterns in multi-field constructors
/// are rejected with a compile error (ADR 18.4.26a).
///
/// The payload of Cons is a product type (T, List<T>), not a sum type.
/// Nested patterns here would silently drop arms, so we emit an error.
#[test]
fn test_cons_multiple_fields_no_nested_match() {
    let errors = elab_err(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn sum_first_two(xs: List<Nat>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(a, Nil()) => a,
                Cons(a, Cons(b, _)) => a + b
            }
        }
    "#,
    );

    assert!(
        errors
            .iter()
            .any(|e| format!("{:?}", e).contains("nested constructor patterns")),
        "Expected nested constructor pattern error, got: {:?}",
        errors,
    );
}

/// Test that multi-field Cons with nested patterns and catch-all
/// is rejected (ADR 18.4.26a). Use explicit inner match instead.
#[test]
fn test_cons_multiple_arms_with_fallback() {
    let errors = elab_err(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn describe_list(xs: List<String>) -> String {
            match xs {
                Nil() => "empty",
                Cons(only, Nil()) => only,
                Cons(first, _) => first
            }
        }
    "#,
    );

    assert!(
        errors
            .iter()
            .any(|e| format!("{:?}", e).contains("nested constructor patterns")),
        "Expected nested constructor pattern error, got: {:?}",
        errors,
    );
}

/// Test that multi-field Cons patterns work when rewritten with
/// explicit inner matches (the recommended fix from ADR 18.4.26a).
#[test]
fn test_cons_explicit_inner_match() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn describe_list(xs: List<String>) -> String {
            match xs {
                Nil() => "empty",
                Cons(first, rest) => match rest {
                    Nil() => first,
                    Cons(_, _) => first
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "describe_list");
}

// ============================================================================
// Single-Field Constructor With Nested Sum (Nested Match Required)
// ============================================================================
//
// These tests verify that single-field constructors whose payload IS a sum
// type correctly build nested matches.

/// Test single-field constructor with Option payload.
///
/// The Wrapper payload is Option<Nat> (a sum type), so nested matching
/// should be built for multiple Wrapper arms.
#[test]
fn test_single_field_wrapper_needs_nested_match() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type Wrapper<T> =
            | Wrap(T)
        
        fn unwrap_option_wrapper(w: Wrapper<Option<Nat>>) -> Nat {
            match w {
                Wrap(None()) => 0,
                Wrap(Some(n)) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unwrap_option_wrapper");
}

/// Test single-field constructor with Result payload.
#[test]
fn test_single_field_box_with_result_payload() {
    let defs = elab_ok(
        r#"
        pub type Result<T, E> =
            | Ok(T)
            | Err(E)
        
        pub type Box<T> =
            | MkBox(T)
        
        fn unbox_result(b: Box<Result<String, Nat>>) -> String {
            match b {
                MkBox(Ok(s)) => s,
                MkBox(Err(_)) => "error"
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unbox_result");
}

// ============================================================================
// Mixed Patterns Tests (Variable + Constructor in Same Match)
// ============================================================================
//
// Tests that combine catch-all/variable patterns with constructor patterns
// in the same match expression.

/// Test mixing variable pattern with nested constructors.
///
/// Variable pattern `x` should match any Option, while specific patterns
/// match None() and Some(_).
#[test]
fn test_variable_and_constructor_patterns_mixed() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn option_priority(opt: Option<Nat>) -> Nat {
            match opt {
                None() => 0,
                Some(n) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test catch-all wildcard with nested option patterns.
#[test]
fn test_wildcard_with_nested_patterns() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        fn match_with_wildcard(opt: Option<Option<Nat>>) -> Nat {
            match opt {
                Some(Some(n)) => n,
                _ => 0
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "match_with_wildcard");
}
