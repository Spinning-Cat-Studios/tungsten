//! Tests for type definitions: type aliases, ADTs, constructors, recursive types, constructor inference.

use super::elab_ok;

// ─────────────────────────────────────────────────────────────────────────────
// Type definitions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_type_alias() {
    // Type aliases don't produce CoreDefs
    let defs = elab_ok("type MyNat = Nat");
    assert!(defs.is_empty());
}

#[test]
fn test_elaborate_nary_constructor() {
    // N-ary constructor application like Some(x)
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn wrap(x: Nat) -> Option<Nat> {
            Some(x)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "wrap");
    // The result type should be Option<Nat> encoded as sum type: Unit + Nat
    // Some(x) should elaborate to: inr(x)
}

#[test]
fn test_elaborate_multi_field_constructor() {
    // Constructor with multiple fields
    let defs = elab_ok(
        r#"
        type Pair<A, B> = MkPair(A, B)
        
        fn make_pair(a: Nat, b: Bool) -> Pair<Nat, Bool> {
            MkPair(a, b)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "make_pair");
    // MkPair(a, b) should elaborate to: (a, b) (product)
}

#[test]
fn test_elaborate_constructor_pattern_match() {
    // Pattern matching on ADT constructors
    // Note: nullary constructors use () syntax in patterns for disambiguation
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn unwrap_or(opt: Option<Nat>, default: Nat) -> Nat {
            match opt {
                None() => default,
                Some(x) => x,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unwrap_or");
    // The match should elaborate to: case(opt, _, default, x, x)
}

#[test]
fn test_elaborate_multi_field_pattern_match() {
    // Pattern matching on multi-field constructors
    let defs = elab_ok(
        r#"
        type Pair<A, B> = MkPair(A, B)
        
        fn first(p: Pair<Nat, Bool>) -> Nat {
            match p {
                MkPair(a, b) => a,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "first");
    // The match should destructure the pair
}

#[test]
fn test_elaborate_recursive_adt_match() {
    // Pattern matching on recursive ADTs (List)
    // This tests unfold being inserted before matching
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        
        fn is_empty(xs: List<Nat>) -> Bool {
            match xs {
                Nil() => true,
                Cons(h, t) => false,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "is_empty");
    // The match should unfold the μ-type before case analysis
}

#[test]
fn test_elaborate_generic_adt_sum_type_arg() {
    // Test for Bug #6: Generic pattern substitution with sum type arg
    // When matching Option<MyKind>, the variable `k` in `Some(k)` should have type MyKind
    let defs = elab_ok(
        r#"
        type MyKind = KindA | KindB(Nat)
        type Option<T> = None | Some(T)
        
        fn get_default_for_kind(opt: Option<MyKind>) -> Nat {
            match opt {
                None() => 0,
                Some(k) => 42
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_default_for_kind");
}

#[test]
fn test_elaborate_generic_adt_nested_match() {
    // Test for Bug #6: Generic pattern substitution
    // When matching Option<MyKind>, the variable `k` in `Some(k)` should have type MyKind
    // Then the nested match on `k` should work correctly
    let defs = elab_ok(
        r#"
        type MyKind = KindA | KindB(Nat)
        type Option<T> = None | Some(T)
        
        fn test_nested_match(opt: Option<MyKind>) -> Nat {
            match opt {
                None() => 0,
                Some(k) => match k {
                    KindA() => 1,
                    KindB(n) => n
                }
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test_nested_match");
}

#[test]
fn test_elaborate_recursive_adt_constructor_return() {
    // Test from ADR 20.1.26.Mu-Type-Unification
    // This was the blocker for Phase 3A Lexer: returning a constructor
    // of a recursive type should unify with the expected return type.
    let defs = elab_ok(
        r#"
        type LexError = MkLexError(String)
        type LexErrors = NoErrors | ConsError(LexError, LexErrors)
        
        fn add_error(errs: LexErrors, err: LexError) -> LexErrors {
            ConsError(err, errs)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "add_error");
    // The key test: ConsError(err, errs) should type-check against LexErrors
    // This requires μ-type α-equivalence to work correctly
}

#[test]
fn test_elaborate_recursive_adt_nil_return() {
    // Returning the nullary constructor of a recursive ADT
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        
        fn empty() -> List<Nat> {
            Nil
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "empty");
}

#[test]
fn test_elaborate_recursive_adt_cons_return() {
    // Returning a non-nullary constructor of a recursive ADT
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        
        fn singleton(x: Nat) -> List<Nat> {
            Cons(x, Nil)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "singleton");
}

#[test]
fn test_elaborate_list_with_head_function() {
    // Test more complex List operations
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        type List<T> = Nil | Cons(T, List<T>)
        
        fn head(xs: List<Nat>) -> Option<Nat> {
            match xs {
                Nil() => None,
                Cons(h, t) => Some(h),
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "head");
}

// ─────────────────────────────────────────────────────────────────────────────
// Constructor type inference
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_constructor_type_inference() {
    // Test that Err(msg) can infer T when checked against Result<T, String>
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        
        fn fail() -> Result<Nat, String> {
            Err("error")
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "fail");
}

#[test]
fn test_elaborate_constructor_type_inference_option_none() {
    // Test that None can infer T when checked against Option<T>
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn nothing() -> Option<Nat> {
            None
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "nothing");
}

#[test]
fn test_elaborate_constructor_type_inference_let_binding() {
    // Test constructor inference in let bindings with type annotations
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        
        fn test() -> Result<Nat, String> {
            let x: Result<Nat, String> = Err("error");
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test");
}
