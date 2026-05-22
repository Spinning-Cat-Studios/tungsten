//! Tests for tuple and block expressions.

use crate::elaborate::tests::elab_ok;
use tungsten_core::{Term, Type};

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
// Tuple Destructuring with Wildcards (ADR 21.4.26e)
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively check whether a Term tree contains `Snd(Var(v))` for a given
/// variable prefix. This verifies the projection targets snd (not fst).
fn contains_snd_of_var(term: &Term, var_prefix: &str) -> bool {
    match term {
        Term::Snd(inner) => {
            matches!(inner.as_ref(), Term::Var(v) if v.starts_with(var_prefix))
                || contains_snd_of_var(inner, var_prefix)
        }
        Term::Fst(inner) => contains_snd_of_var(inner, var_prefix),
        Term::Let(_, _, def, body) => {
            contains_snd_of_var(def, var_prefix) || contains_snd_of_var(body, var_prefix)
        }
        Term::App(f, a) => contains_snd_of_var(f, var_prefix) || contains_snd_of_var(a, var_prefix),
        _ => false,
    }
}

/// Verify `let (_, b) = pair` elaborates correctly.
/// The binding for `b` must project snd (position 1), not fst (position 0).
#[test]
fn test_wildcard_tuple_snd_projection() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (1, 2);
            let (_, b) = p;
            b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
    // The term must contain snd(____tup…) to extract the second element.
    // fresh_var("__tup") produces "____tup0" (prefix __ + __tup + counter).
    assert!(
        contains_snd_of_var(&defs[0].term, "____tup"),
        "Expected snd(____tup…) projection for (_, b) pattern, got: {}",
        defs[0].term
    );
}

/// Verify `let (_, _, c) = triple` projects to the last element.
#[test]
fn test_wildcard_tuple_last_projection() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let t = (1, 2, 3);
            let (_, _, c) = t;
            c
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify `let (a, _, c) = triple` binds first and third correctly.
#[test]
fn test_wildcard_tuple_middle_projection() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let t = (1, 2, 3);
            let (a, _, c) = t;
            a + c
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify `let (a, (_, c)) = pair` handles nested wildcard.
#[test]
fn test_wildcard_nested_tuple_projection() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (1, (2, 3));
            let (a, (_, c)) = p;
            a + c
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify `let (_, _) = pair` produces no crash and returns correct result.
#[test]
fn test_wildcard_tuple_all_wildcards() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (1, 2);
            let (_, _) = p;
            42
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify deeply nested wildcard: `let (a, (_, (d, e))) = complex`.
#[test]
fn test_wildcard_deep_nested_tuple_projection() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (1, (2, (3, 4)));
            let (a, (_, (d, e))) = p;
            a + d + e
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify that a non-wildcard destructuring still works (regression guard).
#[test]
fn test_tuple_destructuring_no_wildcards() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (10, 20);
            let (a, b) = p;
            a + b
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify wildcard at end (trailing position) — this was always correct
/// but serves as a regression guard.
#[test]
fn test_wildcard_trailing_position() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (10, 20);
            let (a, _) = p;
            a
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}

/// Verify tuple destructuring in block statement (not just let expression).
/// This exercises `elab_stmt_let_tuple` as opposed to `elab_let_tuple`.
#[test]
fn test_wildcard_tuple_in_block_statement() {
    let defs = elab_ok(
        r#"
        fn test() -> Nat {
            let p = (5, 15);
            let (_, b) = p;
            let result = b + 1;
            result
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Nat);
}
