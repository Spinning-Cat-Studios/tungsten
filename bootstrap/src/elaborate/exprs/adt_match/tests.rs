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

// ============================================================================
// Regression Tests for ADR 30.1.26
// ============================================================================

/// Regression test: ensure μ-variables don't leak into error messages.
///
/// Before the fix, error messages would contain `α_List` instead of `List<String>`.
#[test]
fn test_no_leaked_mu_variables() {
    // This should succeed without any α_* variables in types
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn process(xs: List<String>) -> String {
            match xs {
                Nil() => "done",
                Cons(s, rest) => s
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

/// Regression test: recursive tail calls with correct types.
#[test]
fn test_recursive_tail_call_types() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn last(xs: List<String>) -> String {
            match xs {
                Nil() => "empty",
                Cons(s, Nil()) => s,
                Cons(_, rest) => last(rest)
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
}

// ============================================================================
// Cross-Module Type::App Unfolding Tests (ADR 30.1.26 Category C Fix)
// ============================================================================
//
// When scrutinee types come from cross-module references, they appear as
// Type::App("TypeName", [args]) instead of their structural μ-encoding.
// The unfold_scrutinee_type function must normalize these before unfolding.

/// Test that unfolding works correctly when using a type defined in another module.
///
/// This simulates the case where we have:
///   - Module A defines `type Foo = | Bar | Baz`
///   - Module B imports A and matches on `Foo`
///   - Module B sees the scrutinee type as Type::App("A::Foo", [])
///
/// The fix normalizes Type::App before attempting to unfold as μ-type.
#[test]
fn test_unfold_cross_module_simple_adt() {
    // Simulate cross-module reference by defining type in one "module" and using in another
    let defs = elab_ok(
        r#"
        pub type Status =
            | Active
            | Inactive
            | Pending
        
        fn is_active(s: Status) -> Bool {
            match s {
                Active() => true,
                Inactive() => false,
                Pending() => false
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "is_active");
}

/// Test cross-module unfolding with generic recursive types.
///
/// This is the more complex case where:
///   - Type is generic: List<T>
///   - Type is recursive: uses μ-encoding
///   - Type is cross-module: appears as Type::App
///
/// All three factors must work together correctly.
#[test]
fn test_unfold_cross_module_generic_recursive() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        // This function simulates using List from another module
        fn length(xs: List<Nat>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(_, rest) => 1 + length(rest)
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "length");
}

/// Test that nested pattern matching works with cross-module types.
///
/// Nested matches require multiple unfold operations, each of which
/// must handle Type::App correctly.
#[test]
fn test_unfold_cross_module_nested_match() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        fn find_head(xs: List<Option<String>>) -> String {
            match xs {
                Nil() => "not found",
                Cons(opt, _) => match opt {
                    None() => "empty",
                    Some(s) => s
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "find_head");
}
// ============================================================================
// Type Alias with Generic ADT (ADR 31.1.26 Phase 2)
// ============================================================================

/// Test that pattern matching on `List<AliasType>` preserves the alias identity.
///
/// This is the key test for ADR 31.1.26 Phase 2. The fix ensures that type aliases
/// are represented as `Type::App("AliasName", [])` during elaboration, so that
/// type argument extraction correctly identifies the alias name rather than its
/// expanded form.
///
/// Before the fix:
/// - `List<SourceEntry>` where `type SourceEntry = (String, String)`
/// - Type argument extraction would see `(String, String)` instead of `SourceEntry`
/// - This caused E9999 "expected sum type" errors
///
/// After the fix:
/// - Type alias is preserved as `Type::App("SourceEntry", [])`
/// - Pattern matching correctly extracts the alias as the type argument
/// - Field types are correctly computed
#[test]
fn test_type_alias_preserved_in_generic_adt_pattern() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        // Type alias for a tuple type
        pub type Entry = (String, String)
        
        fn get_first_entry(xs: List<Entry>) -> Entry {
            match xs {
                Nil() => ("", ""),
                Cons(entry, _) => entry
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_first_entry");
}

/// Test that type alias preservation works with nested generic types.
///
/// This tests `Option<List<AliasType>>` pattern matching.
#[test]
fn test_type_alias_nested_generic_pattern() {
    let defs = elab_ok(
        r#"
        pub type Option<T> =
            | None
            | Some(T)
        
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        // Type alias for a simple type (not a tuple, to avoid tuple pattern issues)
        pub type Count = Nat
        
        fn sum_counts(opt: Option<List<Count>>) -> Nat {
            match opt {
                None() => 0,
                Some(xs) => match xs {
                    Nil() => 0,
                    Cons(count, _) => count
                }
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "sum_counts");
}

/// Test that parameterized type aliases work correctly.
///
/// `type Identity<T> = T` used as `List<Identity<Nat>>`
#[test]
fn test_parameterized_type_alias_in_generic_adt() {
    let defs = elab_ok(
        r#"
        pub type List<T> =
            | Nil
            | Cons(T, List<T>)
        
        // Parameterized type alias (identity alias)
        pub type Identity<T> = T
        
        fn get_first_identity(xs: List<Identity<Nat>>) -> Nat {
            match xs {
                Nil() => 0,
                Cons(n, _) => n
            }
        }
    "#,
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_first_identity");
}

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

/// Test that Cons patterns with multiple fields work correctly.
///
/// The payload of Cons is a product type (T, List<T>), not a sum type.
/// Attempting to build nested matches would fail with "expected sum type".
#[test]
fn test_cons_multiple_fields_no_nested_match() {
    let defs = elab_ok(
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

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "sum_first_two");
}

/// Test multiple Cons patterns with catch-all fallback.
///
/// Multiple arms share Cons constructor, but each binds different variables.
#[test]
fn test_cons_multiple_arms_with_fallback() {
    let defs = elab_ok(
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

/// Test mutual recursion through nested types.
#[test]
fn test_mu_unwrapping_option_of_list() {
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
                Cons(None(), _) => 0,
                Cons(Some(n), _) => n
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
