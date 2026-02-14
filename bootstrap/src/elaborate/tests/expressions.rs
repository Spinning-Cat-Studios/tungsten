//! Tests for expression elaboration: let bindings, if, lambda, operators, strings, tuples, annotations.

use super::elab_ok;
use tungsten_core::{Term, Type};

// ─────────────────────────────────────────────────────────────────────────────
// Let bindings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_let() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let x = 1;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

#[test]
fn test_elaborate_let_with_annotation() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let x: Nat = 42;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

// ─────────────────────────────────────────────────────────────────────────────
// If expressions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_if() {
    let defs = elab_ok(
        r#"
        fn test(b: Bool) -> Nat {
            if b { 1 } else { 0 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::arrow(Type::Bool, Type::Nat));
}

#[test]
fn test_elaborate_if_type_mismatch() {
    let errors = super::elab_err(
        r#"
        fn test(b: Bool) -> Nat {
            if b { true } else { 0 }
        }
    "#,
    );
    assert!(!errors.is_empty());
    // Should complain about type mismatch in branches
}

// ─────────────────────────────────────────────────────────────────────────────
// Lambda expressions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_lambda_checked() {
    let defs = elab_ok(
        r#"
        fn make_const() -> Nat -> Nat {
            |x| x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::arrow(Type::Nat, Type::Nat));
}

#[test]
fn test_elaborate_lambda_inferred() {
    let defs = elab_ok(
        r#"
        fn make_id() -> Nat -> Nat {
            fn(x: Nat) => x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Binary operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_add() {
    let defs = elab_ok(
        r#"
        fn add(x: Nat, y: Nat) -> Nat {
            x + y
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Type should be Nat → Nat → Nat
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::Nat, Type::arrow(Type::Nat, Type::Nat))
    );
}

#[test]
fn test_elaborate_and() {
    let defs = elab_ok(
        r#"
        fn both(a: Bool, b: Bool) -> Bool {
            a && b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_elaborate_or() {
    let defs = elab_ok(
        r#"
        fn either(a: Bool, b: Bool) -> Bool {
            a || b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_elaborate_not() {
    let defs = elab_ok(
        r#"
        fn negate(b: Bool) -> Bool {
            !b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Strings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_string_concat() {
    let defs = elab_ok(
        r#"
        fn greet(name: String) -> String {
            "Hello, " ++ name
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Type should be String → String
    assert_eq!(defs[0].ty, Type::arrow(Type::String, Type::String));
}

#[test]
fn test_elaborate_string_concat_chained() {
    let defs = elab_ok(
        r#"
        fn wrap(s: String) -> String {
            "[" ++ s ++ "]"
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Tuples
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_tuple() {
    let defs = elab_ok(
        r#"
        fn pair() -> Nat * Bool {
            (1, true)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::product(Type::Nat, Type::Bool));
}

#[test]
fn test_elaborate_unit() {
    let defs = elab_ok(
        r#"
        fn nothing() -> Unit {
            ()
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Unit);
    assert_eq!(defs[0].term, Term::Unit);
}

// ─────────────────────────────────────────────────────────────────────────────
// Type annotations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_annotation() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            (0 : Nat)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

// ─────────────────────────────────────────────────────────────────────────────
// Blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_block_with_statements() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let x = 1;
            let y = 2;
            x
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

#[test]
fn test_elaborate_block_ending_with_semicolon_returns_unit() {
    // A block that ends with a semicolon (no trailing expression) has type Unit
    let defs = elab_ok(
        r#"
        fn do_nothing() -> Unit {
            let x = 1;
            let y = 2;
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Unit);
}

#[test]
fn test_elaborate_empty_block_returns_unit() {
    // An empty block {} has type Unit
    let defs = elab_ok(
        r#"
        fn empty() -> Unit {
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Unit);
}

// ─────────────────────────────────────────────────────────────────────────────
// Recursive type unification in branches (ADR 25.1.26, issue 4.3)
// ─────────────────────────────────────────────────────────────────────────────

/// Regression test for ADR 25.1.26 section 4.3: recursive type unification bug.
///
/// The bug occurs when if-branches use a recursive type (like List) at different
/// "depths" of transformation. One branch uses the accumulator directly while
/// another extends it first. The type checker incorrectly fails to unify because
/// the internal μ-type representation differs between branches.
///
/// See: doc/ADRs/in_progress/25.1.26.Bootstrap-Type-Inference-Issues.md
#[test]
fn test_if_branches_with_recursive_type_direct_vs_extended() {
    // This pattern used to fail with:
    // "expected `List<...>`, found `List`"
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        
        fn identity(xs: List<Nat>) -> List<Nat> { xs }
        fn cons_nat(x: Nat, xs: List<Nat>) -> List<Nat> { Cons(x, xs) }
        
        // Critical pattern: branch A uses acc directly, branch B extends acc first
        fn problematic(acc: List<Nat>, done: Bool) -> List<Nat> {
            if done {
                identity(acc)           // Branch A: uses acc directly
            } else {
                let acc2 = cons_nat(1, acc);  // Branch B: extends acc first
                identity(acc2)
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 3); // identity, cons_nat, problematic
    assert_eq!(defs[2].name, "problematic");
}

/// Variant: test that nested if-else with recursive types works.
#[test]
fn test_nested_if_with_recursive_type() {
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        
        fn identity(xs: List<Nat>) -> List<Nat> { xs }
        fn cons_nat(x: Nat, xs: List<Nat>) -> List<Nat> { Cons(x, xs) }
        
        fn nested(acc: List<Nat>, a: Bool, b: Bool) -> List<Nat> {
            if a {
                if b {
                    identity(acc)
                } else {
                    let acc2 = cons_nat(1, acc);
                    identity(acc2)
                }
            } else {
                let acc3 = cons_nat(2, acc);
                let acc4 = cons_nat(3, acc3);
                identity(acc4)
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 3);
    assert_eq!(defs[2].name, "nested");
}

/// Variant: match expression with recursive type in different arms.
#[test]
fn test_match_arms_with_recursive_type() {
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        type Option<T> = None | Some(T)
        
        fn identity(xs: List<Nat>) -> List<Nat> { xs }
        fn cons_nat(x: Nat, xs: List<Nat>) -> List<Nat> { Cons(x, xs) }
        
        fn process(opt: Option<Nat>, acc: List<Nat>) -> List<Nat> {
            match opt {
                None() => identity(acc),  // Direct use
                Some(x) => {
                    let acc2 = cons_nat(x, acc);  // Extended use
                    identity(acc2)
                }
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 3);
    assert_eq!(defs[2].name, "process");
}
