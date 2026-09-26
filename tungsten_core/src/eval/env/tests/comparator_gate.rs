//! `__cmp<T>` resolution and the assertion boundary (ADR 1.8.26b D3).
//!
//! Every case here is about the failure mode that had no error message: a
//! comparison that never ran let the enclosing `assert_eq` skip, so
//! `tungsten test` reported the test `ok`. Each asserts that a distinct way
//! of failing now reaches the reporting boundary as a named stop.
//!
//! Tests: tungsten_core/src/eval/env/handlers/types_and_recursion.rs,
//! tungsten_core/src/eval/env/handlers/externs/call.rs

use crate::eval::env::*;
use crate::types::Type;

#[test]
fn cmp_intrinsic_resolves_via_synth_callback() {
    use std::rc::Rc;
    // Callback synthesizes a trivial "compare_X" whose body is `Zero`.
    let synth: ComparatorSynth = Rc::new(|_ty: &Type| {
        Ok((
            "compare_X".to_string(),
            vec![("compare_X".to_string(), Term::Zero)],
        ))
    });
    let env = EvalEnv::empty().with_comparator_synth(synth);
    // `__cmp<Nat>` → Global("compare_X") → its registered body `Zero`.
    let term = Term::ty_app(
        Term::Global(crate::eval::COMPARE_INTRINSIC.to_string()),
        Type::Nat,
    );
    assert_eq!(eval_with_env(&term, &env), Ok(Term::Zero));
    // The synthesized def is now registered and looked up directly.
    assert_eq!(env.lookup("compare_X"), GlobalLookup::Value(Term::Zero));
}
#[test]
fn cmp_intrinsic_without_callback_reports_rather_than_going_silently_stuck() {
    // ADR 1.8.26b D3: pre-gate this returned the residual as if it were an
    // ordinary value, so the enclosing assertion never ran and the test passed.
    let env = EvalEnv::empty();
    let term = Term::ty_app(
        Term::Global(crate::eval::COMPARE_INTRINSIC.to_string()),
        Type::Nat,
    );
    let Err(EvalStopped::Uncomparable(failure)) = eval_with_env(&term, &env) else {
        panic!("an unresolvable `compare` must not surface as a value");
    };
    assert_eq!(failure.kind, ComparatorFailureKind::NoSynthesizer);
}
#[test]
fn cmp_intrinsic_surfaces_the_callbacks_reason() {
    use std::rc::Rc;
    let synth: ComparatorSynth = Rc::new(|_ty: &Type| {
        Err(ComparatorFailure::new(
            "Alpha",
            ComparatorFailureKind::IncompleteClosure {
                dangling: "compare_Mu_Rotated".to_string(),
                cause: None,
            },
        ))
    });
    let env = EvalEnv::empty().with_comparator_synth(synth);
    let term = Term::ty_app(
        Term::Global(crate::eval::COMPARE_INTRINSIC.to_string()),
        Type::Nat,
    );
    let Err(EvalStopped::Uncomparable(failure)) = eval_with_env(&term, &env) else {
        panic!("the callback's reason must reach the reporting boundary");
    };
    assert_eq!(failure.type_name, "Alpha");
    assert_eq!(
        failure.kind,
        ComparatorFailureKind::IncompleteClosure {
            dangling: "compare_Mu_Rotated".to_string(),
            cause: None,
        }
    );
}
#[test]
fn residual_comparison_reaching_an_assertion_is_a_stop() {
    // The invariant, asserted at the boundary rather than by enumerating the
    // ways synthesis can fail: `assert(compare_is_equal(compare(a, b)))` with
    // an unresolved comparator must not report a pass (ADR 1.8.26b).
    let env = EvalEnv::empty();
    let residual = Term::app(Term::Global("compare_Alpha".to_string()), Term::Zero);
    let t = Term::ExternCall(
        "__c_tg_assert_eq_bool".to_string(),
        vec![residual, Term::True],
    );
    assert_eq!(
        eval_with_env(&t, &env),
        Err(EvalStopped::ComparisonNeverRan {
            symbol: "compare_Alpha".to_string()
        })
    );
}
#[test]
fn a_healthy_assertion_is_untouched_by_the_residual_check() {
    // The non-vacuity twin of the test above: the check inspects only NON-value
    // arguments, so a passing assertion cannot trip it. Without this, a gate
    // that fired on everything would pass the test above just as well.
    let env = EvalEnv::empty();
    let t = Term::ExternCall(
        "__c_tg_assert_eq_bool".to_string(),
        vec![Term::Zero, Term::Zero],
    );
    assert_eq!(eval_with_env(&t, &env), Ok(Term::Unit));
}
#[test]
fn a_residual_that_names_no_comparator_stays_an_ordinary_stuck_term() {
    // Scope guard: the assertion boundary reports comparisons that never ran,
    // not every stuck assertion. An unrelated unresolved global is still the
    // pre-existing (silent) outcome, so this change cannot be blamed for
    // unrelated stuck programs turning red.
    let env = EvalEnv::empty();
    let t = Term::ExternCall(
        "__c_tg_assert_eq_bool".to_string(),
        vec![Term::Global("some_other_global".to_string()), Term::True],
    );
    assert!(matches!(eval_with_env(&t, &env), Ok(_)));
}

#[test]
fn registered_comparators_are_visible_to_a_caller_diagnosing_a_residual() {
    // The false-lead fix (ADR 1.8.26b close-out): a synthesized comparator IS
    // defined, but only in the env's dynamic map. A diagnosis that resolves
    // globals against the static def list alone reports it "never resolved" —
    // which is what sent the D2 investigation after the wrong cause.
    let env = EvalEnv::empty();
    assert!(
        env.registered_comparators().is_empty(),
        "nothing is registered before synthesis runs"
    );

    env.register_comparators(vec![
        ("compare_Nat".to_string(), Term::Zero),
        ("compare_Bool".to_string(), Term::Unit),
    ]);

    let mut names = env.registered_comparators();
    names.sort();
    assert_eq!(names, vec!["compare_Bool", "compare_Nat"]);
}
