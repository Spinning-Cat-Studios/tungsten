//! Regression tests for recursive type unification in branches (ADR 25.1.26, issue 4.3).

use crate::elaborate::tests::elab_ok;
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
