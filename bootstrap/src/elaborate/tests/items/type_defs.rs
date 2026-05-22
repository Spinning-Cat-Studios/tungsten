//! Tests for type definitions: type aliases, ADTs, constructors, recursive types, constructor inference.

use crate::elaborate::tests::elab_ok;
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

// ─────────────────────────────────────────────────────────────────────────────
// Recursiveness consistency (ADR 21.4.26c)
// ─────────────────────────────────────────────────────────────────────────────

/// Full-pipeline test exercising all 5 call sites of `adt_is_recursive`
/// with mutually recursive types. If the debug assertion in `adt_is_recursive`
/// fires, this test panics — proving all callers agree on recursiveness.
///
/// Call sites exercised:
///   1. types/encoding.rs      — ADT encoding (type A, type B)
///   2. types/normalize/adt.rs — normalization during fold wrapping
///   3. exprs/constructors/context.rs — `MkA(LeafB)` constructor usage
///   4. exprs/adt_match/resolution.rs — `match a { ... }` resolution
///   5. exprs/patterns/wrapping.rs    — nested pattern wrapping
#[test]
fn test_mutual_recursion_consistency_all_call_sites() {
    let defs = elab_ok(
        r#"
        type A = MkA(B) | LeafA
        type B = MkB(A) | LeafB

        fn make_a() -> A {
            MkA(LeafB)
        }

        fn is_leaf_a(a: A) -> Bool {
            match a {
                LeafA() => true,
                MkA(b) => false,
            }
        }
    "#,
    );
    // Two functions should elaborate successfully.
    // If adt_is_recursive disagrees across call sites, the debug assertion
    // in encoding_utils.rs fires and this test panics.
    assert_eq!(defs.len(), 2);
}

/// Full-pipeline test with nested pattern matching on mutually recursive types.
/// Exercises call site 5 (patterns/wrapping.rs) more deeply via nested
/// constructor destructuring: `Branch(Cons(Leaf(n), _)) => n`.
#[test]
fn test_mutual_recursion_nested_pattern_consistency() {
    let defs = elab_ok(
        r#"
        type Tree = Leaf(Nat) | Branch(Forest)
        type Forest = Empty | Cons(Tree, Forest)

        fn tree_value(t: Tree) -> Nat {
            match t {
                Leaf(n) => n,
                Branch(f) => forest_first(f),
            }
        }

        fn forest_first(f: Forest) -> Nat {
            match f {
                Empty() => 0,
                Cons(t, rest) => tree_value(t),
            }
        }
    "#,
    );
    // Both functions should elaborate without triggering the consistency assertion.
    assert_eq!(defs.len(), 2);
}

/// Full-pipeline test with a non-recursive flat enum exercising constructors + match.
/// All 5 call sites should agree that this type is NOT recursive.
#[test]
fn test_non_recursive_adt_consistency_all_call_sites() {
    let defs = elab_ok(
        r#"
        type Color = Red | Green | Blue

        fn make_color() -> Color {
            Green
        }

        fn is_red(c: Color) -> Bool {
            match c {
                Red() => true,
                Green() => false,
                Blue() => false,
            }
        }
    "#,
    );
    // Both functions should elaborate without triggering the consistency assertion.
    // All call sites should agree on is_recursive == false.
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_nullary_constructor_pattern_without_parens() {
    // Nullary constructors like `Zero` in patterns should be recognized as constructors,
    // not variable bindings. Without this fix, `Zero => Zero` would bind `Zero` as a
    // variable of type `Unit` (the payload of the nullary constructor), shadowing the
    // constructor and causing a type mismatch.
    let defs = elab_ok(
        r#"
        type Peano = Zero | Succ(Peano)

        fn identity(p: Peano) -> Peano {
            match p {
                Zero => Zero,
                Succ(n) => Succ(n),
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "identity");
}
