//! ADT match tests for μ-type unwrapping and sum component navigation.

use crate::elaborate::tests::{elab_err, elab_ok};

// ============================================================================
// Mu Type Unwrapping Tests
// ============================================================================
//
// These tests verify that Mu types are correctly unwrapped before
// attempting to navigate Sum structures. The fix added unwrap_mu() calls
// in get_sum_component and build_ctor_extraction_at.

/// Test recursive type pattern matching (requires Mu unwrapping).
///
/// List<T> is encoded as μ. Sum(Unit, T × α) where α is the Mu variable.
/// Matching on Cons requires unwrapping this Mu type.
#[test]
fn test_mu_unwrapping_basic_list() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn first_or_zero(xs: List<Nat>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(n, _) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test deeply nested recursive types (multiple Mu unwrappings).
#[test]
fn test_mu_unwrapping_nested_recursive() {
    let defs = elab_ok(
        r#"
        pub type Tree<T> =
            | Leaf(T)
            | Node(Tree<T>, Tree<T>)
        
        fn leftmost(t: Tree<Nat>) -> Nat {
            match t {
                Leaf(n) => n,
                Node(left, _) => leftmost(left)
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "leftmost");
}

/// Test that nested constructor patterns in multi-field Cons are rejected
/// even when the nested pattern is on a different sum type (ADR 18.4.26a).
#[test]
fn test_mu_unwrapping_option_of_list() {
    let errors = elab_err(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn optional_head(xs: List<Option<Nat>>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(None(), _) => 0,
                Cons(Some(n), _) => n
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

/// Test that unwrapping Option inside List works with explicit inner match.
#[test]
fn test_mu_unwrapping_option_of_list_explicit() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn optional_head(xs: List<Option<Nat>>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(head, _) => match head {
                    None() => 0,
                    Some(n) => n
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "optional_head");
}

// ============================================================================
// Sum Component Navigation Tests
// ============================================================================
//
// Tests for get_sum_component's ability to navigate binary sum structures.
// These test the helper methods: navigate_sum_to_index, step_right_in_sum,
// extract_sum_component_at_position, etc.

/// Test ADT with exactly 2 constructors (binary sum).
///
/// For n=2, the encoding is Sum(A, B) directly.
#[test]
fn test_binary_sum_two_constructors() {
    let defs = elab_ok(
        r#"
        pub type Either<L, R> =
            | Left(L)
            | Right(R)
        
        fn is_left(e: Either<Nat, String>) -> Bool {
            match e {
                Left(_) => true,
                Right(_) => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test ADT with 3 constructors (nested binary sum).
///
/// For n=3, the encoding is Sum(A, Sum(B, C)).
#[test]
fn test_sum_three_constructors() {
    let defs = elab_ok(
        r#"
        pub type Color =
            | Red
            | Green
            | Blue
        
        fn color_code(c: Color) -> Nat {
            match c {
                Red() => 1,
                Green() => 2,
                Blue() => 3
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test ADT with 4 constructors (deeper nesting).
///
/// For n=4, the encoding is Sum(A, Sum(B, Sum(C, D))).
#[test]
fn test_sum_four_constructors() {
    let defs = elab_ok(
        r#"
        pub type Direction =
            | North
            | South
            | East
            | West
        
        fn is_vertical(d: Direction) -> Bool {
            match d {
                North() => true,
                South() => true,
                East() => false,
                West() => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test single-constructor ADT (no sum, just payload).
///
/// For n=1, there's no Sum wrapper - the type IS the payload.
#[test]
fn test_single_constructor_adt() {
    let defs = elab_ok(
        r#"
        pub type Wrapper<T> =
            | Wrap(T)
        
        fn unwrap(w: Wrapper<Nat>) -> Nat {
            match w {
                Wrap(n) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Test ADT with many constructors (flat ADT optimization).
///
/// For n>=3 constructors, the elaborator may use flat ADT lookup
/// (Type::Variant) instead of navigating nested sums.
#[test]
fn test_many_constructors_flat_adt() {
    let defs = elab_ok(
        r#"
        pub type Weekday =
            | Monday
            | Tuesday
            | Wednesday
            | Thursday
            | Friday
            | Saturday
            | Sunday
        
        fn is_weekend(d: Weekday) -> Bool {
            match d {
                Saturday() => true,
                Sunday() => true,
                _ => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}
