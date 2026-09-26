//! Whether an argument shrinks: the strict-subterm rule at whole-group scale,
//! indirect recursion, and the Phase-1 supported inductive subset.
//!
//! *Which* parameter is required to shrink — inference, mutual groups, the
//! `#[decreasing(arg)]` annotation — is `selection_tests`.

use crate::terms::termination::descent::is_supported_inductive;
use crate::terms::termination::{analyze, AdmissionState, FailureReason};
use crate::terms::{Term, TermSpan};
use crate::types::Type;

use super::fixtures::{
    call, list_fn, list_to_nat, list_type, match_list, sole_failure, structural_len, var, views,
    Def,
};

#[test]
fn structural_recursion_is_admitted_as_total() {
    let defs = [structural_len()];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert_eq!(report.state_of("len"), AdmissionState::Total);
    assert!(report.state_of("len").is_delta_reducible());
    assert_eq!(report.recursive_groups, vec![vec!["len".to_string()]]);
}

#[test]
fn a_non_recursive_definition_needs_no_decreasing_parameter() {
    let defs = [
        structural_len(),
        Def::new(
            "wrapper",
            list_to_nat(),
            list_fn("l", call("len", vec![var("l")])),
        ),
    ];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert_eq!(report.state_of("wrapper"), AdmissionState::Total);
    assert_eq!(report.recursive_groups, vec![vec!["len".to_string()]]);
}

#[test]
fn a_nullary_self_reference_has_nothing_that_could_decrease() {
    // `fn diverge() -> Void { diverge() }` elaborates to a bare self-reference.
    let defs = [Def::new(
        "diverge",
        Type::Void,
        Term::Global("diverge".to_string()),
    )];

    match sole_failure(&defs, "diverge") {
        FailureReason::IndirectOccurrence { member } => assert_eq!(member, "diverge"),
        other => panic!("unexpected reason: {other:?}"),
    }
    assert_eq!(
        analyze(&views(&defs)).state_of("diverge"),
        AdmissionState::Rejected
    );
}

#[test]
fn a_self_call_that_never_shrinks_is_rejected() {
    let defs = [Def::new(
        "spin",
        list_to_nat(),
        list_fn("l", call("spin", vec![var("l")])),
    )];

    // Both polarities of the clean/rejecting verdict, so a report that always
    // claimed to be clean would not pass here.
    assert!(!analyze(&views(&defs)).is_clean());

    match sole_failure(&defs, "spin") {
        FailureReason::NoDescent {
            parameter,
            callee,
            argument,
        } => {
            assert_eq!(parameter, "l");
            assert_eq!(callee, "spin");
            assert_eq!(argument, "l");
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn reconstructing_the_scrutinee_is_rejected() {
    // `Cons(0, t)` is not a strict subterm of `l`, even though `t` is.
    let rebuilt = Term::Fold(
        list_type(),
        Box::new(Term::Inr(
            Type::Sum(Box::new(Type::Unit), Box::new(list_type())),
            Box::new(Term::Pair(Box::new(Term::Zero), Box::new(var("t")))),
        )),
    );
    let defs = [Def::new(
        "grow",
        list_to_nat(),
        list_fn(
            "l",
            match_list(var("l"), Term::Zero, "t", call("grow", vec![rebuilt])),
        ),
    )];

    assert!(matches!(
        sole_failure(&defs, "grow"),
        FailureReason::NoDescent { .. }
    ));
}

#[test]
fn a_parameter_of_an_unsupported_type_is_no_candidate() {
    let defs = [Def::new(
        "count",
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        Term::Lambda(
            "n".to_string(),
            Type::Nat,
            Box::new(call(
                "count",
                vec![Term::NatSub(
                    Box::new(var("n")),
                    Box::new(Term::Succ(Box::new(Term::Zero))),
                )],
            )),
        ),
    )];

    match sole_failure(&defs, "count") {
        FailureReason::NoCandidateParameter { parameters } => {
            // The rejection carries the *reason*, not just the name (ADR
            // 12.8.26a) — asserting only the name would pass against the
            // pre-12.8.26a shape and prove nothing about what a reader is told.
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "n");
            assert_eq!(parameters[0].rendered_type, "Nat");
            assert!(
                parameters[0].because.contains("primitive"),
                "{}",
                parameters[0].because
            );
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn the_supported_subset_is_the_two_adt_encodings() {
    let no_own_parameters: [String; 0] = [];
    assert!(is_supported_inductive(&list_type(), &no_own_parameters));
    assert!(is_supported_inductive(
        &Type::Adt("T".to_string(), vec![], vec![]),
        &no_own_parameters
    ));
    for unsupported in [
        Type::Nat,
        Type::String,
        Type::Bool,
        Type::Ref(Box::new(Type::Nat)),
        Type::App("Vec".to_string(), vec![Type::Nat]),
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
    ] {
        assert!(
            !is_supported_inductive(&unsupported, &no_own_parameters),
            "{unsupported}"
        );
    }
}

/// ADR 11.8.26b: a bare `TyVar` is a descent root exactly when it is *not* one
/// of the definition's own type parameters.
///
/// Both directions matter and they are the same call: the cluster-member marker
/// a mutual type family leaves behind (`TyVar("FieldT")`) has to be admitted, and
/// the abstract `T` of a generic function has to keep being refused. A predicate
/// that ignored `own_type_parameters` would pass the first assertion and fail
/// the second.
#[test]
fn a_bare_tyvar_is_a_root_only_when_it_is_not_a_type_parameter() {
    let own_type_parameters = ["T".to_string()];
    assert!(
        is_supported_inductive(&Type::TyVar("FieldT".to_string()), &own_type_parameters),
        "an unbound TyVar names a nominal cluster member"
    );
    assert!(
        !is_supported_inductive(&Type::TyVar("T".to_string()), &own_type_parameters),
        "a TyVar bound by the definition's own TyAbs is abstract"
    );
    let no_own_parameters: [String; 0] = [];
    assert!(
        is_supported_inductive(&Type::TyVar("T".to_string()), &no_own_parameters),
        "the same name is nominal in a definition that binds no type parameters"
    );
}

#[test]
fn indirect_recursion_through_a_binding_is_rejected() {
    // `let g = ind; g(l)` — no call-position occurrence of `ind` at all.
    let body = Term::Let(
        "g".to_string(),
        list_to_nat(),
        Box::new(Term::Global("ind".to_string())),
        Box::new(call("g", vec![var("l")])),
    );
    let defs = [Def::new("ind", list_to_nat(), list_fn("l", body))];

    match sole_failure(&defs, "ind") {
        FailureReason::IndirectOccurrence { member } => assert_eq!(member, "ind"),
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn recursion_through_a_higher_order_argument_is_rejected() {
    // `apply(rec, l)` — `rec` escapes into a function argument.
    let defs = [
        Def::new(
            "rec",
            list_to_nat(),
            list_fn(
                "l",
                call("apply", vec![Term::Global("rec".to_string()), var("l")]),
            ),
        ),
        Def::new(
            "apply",
            Type::Arrow(Box::new(list_to_nat()), Box::new(list_to_nat())),
            Term::Lambda(
                "f".to_string(),
                list_to_nat(),
                Box::new(list_fn("l", call("f", vec![var("l")]))),
            ),
        ),
    ];

    assert!(matches!(
        sole_failure(&defs, "rec"),
        FailureReason::IndirectOccurrence { .. }
    ));
}

#[test]
fn a_failure_points_at_the_offending_call_site() {
    let spanned = Term::Spanned(
        Box::new(call("spin", vec![var("l")])),
        TermSpan::new(31, 40),
    );
    let defs = [Def::new("spin", list_to_nat(), list_fn("l", spanned))];
    let report = analyze(&views(&defs));

    assert_eq!(report.failures[0].span, Some(TermSpan::new(31, 40)));
    assert_eq!(report.failures[0].group, vec!["spin".to_string()]);
}

/// ADR 12.8.26a: `describe_roots` reports every parameter with a verdict, in
/// order — the data behind `info def --why-not-certified`.
///
/// The `vec![]` mutant is what this exists to kill: an empty answer renders as
/// "the definition takes no parameters", which reads plausibly for a command
/// nobody asserts on.
#[test]
fn describe_roots_reports_every_parameter_with_its_verdict() {
    use crate::terms::termination::describe_roots;

    // `fn f(l: Lst, n: Nat)` — one eligible root, one not.
    let term = list_fn(
        "l",
        Term::Lambda("n".to_string(), Type::Nat, Box::new(var("l"))),
    );
    let roots = describe_roots(&term);

    assert_eq!(roots.len(), 2, "both parameters are reported");
    assert_eq!(roots[0].parameter, "l", "in signature order");
    assert!(
        roots[0].ineligible_because.is_none(),
        "a μ-encoded list is a candidate"
    );
    assert_eq!(roots[1].parameter, "n");
    assert!(
        roots[1]
            .ineligible_because
            .is_some_and(|why| why.contains("primitive")),
        "and `Nat` is refused, with the reason: {:?}",
        roots[1].ineligible_because
    );
    assert_eq!(roots[1].rendered_type, "Nat");
}

/// A generic definition's own type parameter stays refused, and the same name
/// unbound does not — the discriminator, exercised through the public entry
/// point rather than the predicate.
#[test]
fn describe_roots_separates_a_type_parameter_from_a_nominal_marker() {
    use crate::terms::termination::describe_roots;

    let abstract_t = Term::TyAbs(
        "T".to_string(),
        Box::new(Term::Lambda(
            "x".to_string(),
            Type::TyVar("T".to_string()),
            Box::new(var("x")),
        )),
    );
    let roots = describe_roots(&abstract_t);
    assert!(
        roots[0]
            .ineligible_because
            .is_some_and(|why| why.contains("type parameter")),
        "{:?}",
        roots[0].ineligible_because
    );

    let nominal = Term::Lambda(
        "x".to_string(),
        Type::TyVar("FieldT".to_string()),
        Box::new(var("x")),
    );
    assert!(
        describe_roots(&nominal)[0].ineligible_because.is_none(),
        "an unbound TyVar is a cluster marker, and a candidate"
    );
}
