//! Reachability tests for the codes ADR 15.8.26b minted or folded out of
//! `ElabErrorKind::Other` — each error class asserted here was demonstrated
//! reachable from parsed input before its code was assigned.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::elab_err;

/// E0022, check mode: the ADT-match codegen path knows which ADT the arms
/// belong to.
#[test]
fn match_scrutinee_not_adt_in_check_mode() {
    let errors = elab_err(
        r#"
        type Opt = None | Some(Nat)
        fn f(n: Nat) -> Nat {
            match n {
                Some(x) => x,
                _ => 0,
            }
        }
    "#,
    );
    let err = errors
        .iter()
        .find(|e| matches!(e.kind, ElabErrorKind::MatchScrutineeNotAdt { .. }))
        .expect("expected a MatchScrutineeNotAdt error");
    assert_eq!(err.kind.code(), "E0022");
    assert!(
        err.message.contains("`Opt`"),
        "check mode knows the arms' ADT: {}",
        err.message
    );
}

/// E0022, infer mode: the sum walkers know only the scrutinee's type.
#[test]
fn match_scrutinee_not_adt_in_infer_mode() {
    let errors = elab_err(
        r#"
        type Opt = None | Some(Nat)
        fn f(n: Nat) -> Nat {
            let y = match n {
                Some(x) => x,
                _ => 0,
            };
            y
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::MatchScrutineeNotAdt { .. })),
        "expected a MatchScrutineeNotAdt error, got: {:?}",
        errors
    );
}

/// E0048: a theorem body is an expression, not a function body, so `return`
/// has no return type to check against.
#[test]
fn return_outside_function_in_theorem_body() {
    let errors = elab_err("theorem t : Nat = return 5");
    let err = errors
        .iter()
        .find(|e| matches!(e.kind, ElabErrorKind::ReturnOutsideFunction))
        .expect("expected a ReturnOutsideFunction error");
    assert_eq!(err.kind.code(), "E0048");
}

/// E0080: `__compare` at a function type, which has no decidable equality.
#[test]
fn comparator_unavailable_for_function_type() {
    let errors = elab_err(
        r#"
        fn f(x: Nat) -> Nat { x }
        fn g(x: Nat) -> Nat { x }
        fn test_fns() -> Bool {
            __compare(f, g)
        }
    "#,
    );
    let err = errors
        .iter()
        .find(|e| matches!(e.kind, ElabErrorKind::ComparatorUnavailable(_)))
        .expect("expected a ComparatorUnavailable error");
    assert_eq!(err.kind.code(), "E0080");
    assert!(
        err.message.contains("no comparator available"),
        "message names the condition: {}",
        err.message
    );
}

/// E0014 fold: destructuring a non-tuple reports ExpectedType, not E9999.
#[test]
fn tuple_destructure_of_non_tuple_is_expected_type() {
    let errors = elab_err(
        r#"
        fn f() -> Nat {
            let (a, b) = 5;
            a
        }
    "#,
    );
    let err = errors
        .iter()
        .find(|e| matches!(e.kind, ElabErrorKind::ExpectedType { .. }))
        .expect("expected an ExpectedType error");
    assert_eq!(err.kind.code(), "E0014");
    assert!(
        err.message.contains("tuple with 2 elements"),
        "message names the expected shape: {}",
        err.message
    );
}

/// E0022 with the failing constructor at index 0, so the sum walker fails in
/// `extract_left_from_sum` rather than `step_right_in_sum`. (The first arm
/// must be a *parenthesised* constructor: a nullary one parses as a Var
/// pattern and surfaces the uncoded "expected constructor pattern" instead.)
#[test]
fn match_scrutinee_not_adt_on_first_constructor_arm() {
    let errors = elab_err(
        r#"
        type Pair = A(Nat) | B(Nat)
        fn f(n: Nat) -> Nat {
            let y = match n {
                A(x) => x,
                B(x) => x,
            };
            y
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::MatchScrutineeNotAdt { .. })),
        "expected a MatchScrutineeNotAdt error, got: {:?}",
        errors
    );
}

/// E9998 from the one internal invariant that is callable directly: an
/// injection for an ADT with zero constructors, which a resolved constructor
/// makes impossible from source.
#[test]
fn empty_adt_injection_is_an_internal_error() {
    use crate::elaborate::Elaborator;
    use tungsten_core::{Context, Term, Type};

    let ctx = Box::leak(Box::new(Context::new()));
    let elaborator = Elaborator::new(ctx);
    let err = elaborator
        .build_constructor_injection(Term::var("x"), 0, 0, &Type::Nat)
        .expect_err("zero constructors must be rejected");
    assert!(matches!(err.kind, ElabErrorKind::InternalError(_)));
    assert_eq!(err.kind.code(), "E9998");
}
