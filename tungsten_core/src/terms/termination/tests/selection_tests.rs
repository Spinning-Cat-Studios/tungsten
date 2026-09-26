//! Decreasing-parameter selection: mutual groups, inference, and the
//! `#[decreasing(arg)]` annotation.
//!
//! Split from `descent_tests` along the question each half answers. That file
//! asks *does this argument shrink*; this one asks *which parameter is the one
//! that has to*, which is where mutual recursion and the annotation live.

use crate::terms::termination::descent::exceeds_search_ceiling;
use crate::terms::termination::{analyze, FailureReason};
use crate::terms::Term;
use crate::types::Type;

use super::fixtures::{
    call, list_fn, list_to_nat, list_type, match_list, sole_failure, structural_len, var, views,
    Def,
};

#[test]
fn mutual_recursion_descending_on_each_caller_root_is_admitted() {
    let member = |name: &str, other: &str| {
        Def::new(
            name,
            Type::Arrow(Box::new(list_type()), Box::new(Type::Bool)),
            list_fn(
                "l",
                match_list(var("l"), Term::True, "t", call(other, vec![var("t")])),
            ),
        )
    };
    let defs = [member("even_len", "odd_len"), member("odd_len", "even_len")];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert_eq!(
        report.recursive_groups,
        vec![vec!["even_len".to_string(), "odd_len".to_string()]]
    );
}

#[test]
fn a_mutual_group_that_hands_the_whole_value_on_is_rejected() {
    // `even_len` descends, `odd_len` passes its root straight back — the group
    // makes no net progress, and descent relative to the caller's root sees it.
    let defs = [
        Def::new(
            "even_len",
            Type::Arrow(Box::new(list_type()), Box::new(Type::Bool)),
            list_fn(
                "l",
                match_list(var("l"), Term::True, "t", call("odd_len", vec![var("t")])),
            ),
        ),
        Def::new(
            "odd_len",
            Type::Arrow(Box::new(list_type()), Box::new(Type::Bool)),
            list_fn("m", call("even_len", vec![var("m")])),
        ),
    ];

    assert!(matches!(
        sole_failure(&defs, "odd_len"),
        FailureReason::NoDescent { .. }
    ));
}

#[test]
fn descent_must_be_relative_to_the_callers_root_not_the_callees_position() {
    // `hand_off` supplies `other` — a parameter of the right *type*, in the
    // right *position*, and no smaller than anything.
    let defs = [
        Def::new(
            "walk",
            Type::Arrow(
                Box::new(list_type()),
                Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
            ),
            list_fn(
                "l",
                list_fn(
                    "other",
                    match_list(
                        var("l"),
                        Term::Zero,
                        "t",
                        call("hand_off", vec![var("t"), var("other")]),
                    ),
                ),
            ),
        ),
        Def::new(
            "hand_off",
            Type::Arrow(
                Box::new(list_type()),
                Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
            ),
            list_fn("a", list_fn("b", call("walk", vec![var("b"), var("a")]))),
        ),
    ];

    assert!(matches!(
        sole_failure(&defs, "hand_off"),
        FailureReason::NoDescent { .. }
    ));
}

#[test]
fn two_decreasing_positions_demand_an_annotation() {
    let pair_fn = |body: Term| list_fn("a", list_fn("b", body));
    let inner = match_list(
        var("b"),
        Term::Zero,
        "u",
        call("both", vec![var("t"), var("u")]),
    );
    let defs = [Def::new(
        "both",
        Type::Arrow(
            Box::new(list_type()),
            Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
        ),
        pair_fn(match_list(var("a"), Term::Zero, "t", inner)),
    )];

    match sole_failure(&defs, "both") {
        FailureReason::AmbiguousDecreasing { candidates } => {
            assert_eq!(candidates, vec!["a".to_string(), "b".to_string()]);
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn an_annotation_resolves_the_ambiguity() {
    let inner = match_list(
        var("b"),
        Term::Zero,
        "u",
        call("both", vec![var("t"), var("u")]),
    );
    let defs = [Def::new(
        "both",
        Type::Arrow(
            Box::new(list_type()),
            Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
        ),
        list_fn(
            "a",
            list_fn("b", match_list(var("a"), Term::Zero, "t", inner)),
        ),
    )
    .decreasing("b")];

    assert!(analyze(&views(&defs)).is_clean());
}

#[test]
fn an_annotation_that_does_not_hold_is_rejected() {
    let defs = [Def::new(
        "keep",
        Type::Arrow(
            Box::new(list_type()),
            Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
        ),
        list_fn(
            "a",
            list_fn(
                "b",
                match_list(
                    var("a"),
                    Term::Zero,
                    "t",
                    call("keep", vec![var("t"), var("b")]),
                ),
            ),
        ),
    )
    .decreasing("b")];

    match sole_failure(&defs, "keep") {
        FailureReason::NoDescent {
            parameter,
            argument,
            ..
        } => {
            assert_eq!(parameter, "b");
            assert_eq!(argument, "b");
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn an_annotation_naming_a_non_parameter_is_rejected() {
    let defs = [structural_len().decreasing("nope")];

    match sole_failure(&defs, "len") {
        FailureReason::UnknownDecreasingParameter {
            annotated,
            parameters,
        } => {
            assert_eq!(annotated, "nope");
            assert_eq!(parameters, vec!["l".to_string()]);
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn an_annotation_on_an_unsupported_parameter_type_is_rejected() {
    let defs = [Def::new(
        "mixed",
        Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
        ),
        Term::Lambda(
            "n".to_string(),
            Type::Nat,
            Box::new(list_fn(
                "l",
                match_list(
                    var("l"),
                    Term::Zero,
                    "t",
                    call("mixed", vec![var("n"), var("t")]),
                ),
            )),
        ),
    )
    .decreasing("n")];

    match sole_failure(&defs, "mixed") {
        FailureReason::UnsupportedInductive { parameter, .. } => assert_eq!(parameter, "n"),
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn an_under_applied_recursive_call_cannot_descend() {
    // `shorted(t)` supplies position 0 but never position 1, which is where
    // the annotation says the decrease lives.
    let defs = [Def::new(
        "shorted",
        Type::Arrow(
            Box::new(list_type()),
            Box::new(Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))),
        ),
        list_fn(
            "a",
            list_fn(
                "b",
                match_list(var("b"), Term::Zero, "t", call("shorted", vec![var("t")])),
            ),
        ),
    )
    .decreasing("b")];

    match sole_failure(&defs, "shorted") {
        FailureReason::PartialApplication { callee, position } => {
            assert_eq!(callee, "shorted");
            assert_eq!(position, 1);
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn the_assignment_search_has_a_ceiling_and_it_is_exact() {
    // 4096 assignments is the largest the checker enumerates; 4097 is not.
    let four_options = || vec![0, 1, 2, 3];
    let six_members: Vec<Vec<usize>> = (0..6).map(|_| four_options()).collect();
    assert_eq!(
        six_members.iter().map(Vec::len).product::<usize>(),
        4096,
        "the fixture must sit exactly on the boundary"
    );
    assert!(!exceeds_search_ceiling(&six_members));

    let mut one_over = six_members.clone();
    one_over.push(vec![0, 1]);
    assert!(exceeds_search_ceiling(&one_over));
}

/// The real corpus shape, and the one the arithmetic used to get wrong.
///
/// ADR 11.8.26b found this by *panicking*: `exceeds_search_ceiling` computed a
/// plain `product()`, and the self-hosted elaborator's 202-member SCC overflows
/// a `usize` the moment its members have two candidates each. Debug panics;
/// release **wraps**, which is the dangerous half — 2^202 mod 2^64 is 0, so a
/// group astronomically past the ceiling would have compared `0 > 4096` as
/// false and been enumerated instead of rejected.
///
/// The boundary test above cannot see this: 4096 and 4097 both fit in a `usize`
/// comfortably, so it passes against the overflowing implementation too.
#[test]
fn a_group_whose_assignment_product_overflows_a_usize_still_exceeds_the_ceiling() {
    // 202 members × 2 candidates = 2^202. The count is the elaborator's real
    // `apply_args` SCC, not a round number chosen to look dramatic.
    let overflowing: Vec<Vec<usize>> = (0..202).map(|_| vec![0, 1]).collect();
    assert!(
        exceeds_search_ceiling(&overflowing),
        "a 2^202 product is past the ceiling however it is computed"
    );

    // And 2^64 exactly, where a wrapping product lands on 0 — the value that
    // would compare *below* the ceiling and be enumerated.
    let exactly_wrapping: Vec<Vec<usize>> = (0..64).map(|_| vec![0, 1]).collect();
    assert!(
        exceeds_search_ceiling(&exactly_wrapping),
        "2^64 wraps to 0; saturation is what keeps this true"
    );
}

#[test]
fn a_single_member_never_exceeds_the_ceiling_on_a_realistic_signature() {
    // The ceiling exists for mutual groups; one function's parameter list
    // cannot reach it, and the check must not fire on ordinary input.
    assert!(!exceeds_search_ceiling(&[vec![0, 1, 2, 3, 4]]));
    assert!(!exceeds_search_ceiling(&[vec![0]]));
    assert!(!exceeds_search_ceiling(&[]));
}
