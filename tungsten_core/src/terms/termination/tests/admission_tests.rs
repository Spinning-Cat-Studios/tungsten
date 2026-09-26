//! `#[partial]` taint, admission states, and the proof boundary.

use crate::terms::termination::{analyze, AdmissionState, FailureReason};
use crate::terms::Term;
use crate::types::Type;

use super::fixtures::{call, list_fn, list_to_nat, list_type, var, views, Def};

/// A self-recursive definition that cannot pass the check.
fn spin(name: &str) -> Def {
    Def::new(
        name,
        list_to_nat(),
        list_fn("l", call(name, vec![var("l")])),
    )
}

#[test]
fn partial_suppresses_the_check_and_admits_the_definition_opaquely() {
    let defs = [spin("spin").partial()];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert_eq!(report.state_of("spin"), AdmissionState::Partial);
    assert!(!report.state_of("spin").is_delta_reducible());
    assert!(!report.state_of("spin").usable_in_proofs());
}

#[test]
fn taint_reaches_a_wrapper_that_only_calls_a_partial_definition() {
    let defs = [
        spin("spin").partial(),
        Def::new(
            "wrapper",
            list_to_nat(),
            list_fn("l", call("spin", vec![var("l")])),
        ),
    ];
    let report = analyze(&views(&defs));

    assert_eq!(report.state_of("wrapper"), AdmissionState::Partial);
    assert_eq!(
        report.tainted,
        vec!["spin".to_string(), "wrapper".to_string()]
    );
    // Executable code may depend on a partial definition — no error here.
    assert!(report.is_clean(), "{:?}", report.failures);
}

#[test]
fn taint_is_transitive_across_several_hops() {
    let hop = |name: &str, target: &str| {
        Def::new(
            name,
            list_to_nat(),
            list_fn("l", call(target, vec![var("l")])),
        )
    };
    let defs = [
        spin("spin").partial(),
        hop("one", "spin"),
        hop("two", "one"),
        hop("three", "two"),
    ];
    let report = analyze(&views(&defs));

    for name in ["one", "two", "three"] {
        assert_eq!(report.state_of(name), AdmissionState::Partial, "{name}");
    }
}

#[test]
fn an_untainted_definition_stays_total() {
    let defs = [
        spin("spin").partial(),
        Def::new("pure", Type::Nat, Term::Zero),
    ];

    assert_eq!(
        analyze(&views(&defs)).state_of("pure"),
        AdmissionState::Total
    );
}

#[test]
fn a_proof_that_mentions_a_partial_constant_is_rejected() {
    let defs = [
        spin("spin").partial(),
        Def::new("thm", Type::Prop, call("spin", vec![Term::Unit])).proof(),
    ];
    let report = analyze(&views(&defs));

    match &report.failures[0].reason {
        FailureReason::PartialInProof { tainted, via } => {
            assert_eq!(tainted, "spin");
            assert!(via.is_empty(), "{via:?}");
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn a_total_looking_wrapper_does_not_launder_taint_into_a_proof() {
    let defs = [
        spin("spin").partial(),
        Def::new(
            "wrapper",
            list_to_nat(),
            list_fn("l", call("spin", vec![var("l")])),
        ),
        Def::new("thm", Type::Prop, call("wrapper", vec![Term::Unit])).proof(),
    ];
    let report = analyze(&views(&defs));

    match &report.failures[0].reason {
        FailureReason::PartialInProof { tainted, via } => {
            assert_eq!(tainted, "spin");
            assert_eq!(via, &vec!["wrapper".to_string()]);
        }
        other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn a_theorem_statement_is_scanned_as_well_as_its_proof_term() {
    // The proof term is trivial; the *type* mentions the partial constant.
    let statement = Type::Eq(
        Box::new(Type::Nat),
        Box::new(Term::Global("spin".to_string())),
        Box::new(Term::Zero),
    );
    let defs = [
        spin("spin").partial(),
        Def::new("thm", statement, Term::Unit).proof(),
    ];
    let report = analyze(&views(&defs));

    assert!(matches!(
        report.failures[0].reason,
        FailureReason::PartialInProof { .. }
    ));
}

#[test]
fn a_proof_over_total_definitions_is_admitted() {
    let defs = [
        Def::new("zero", Type::Nat, Term::Zero),
        Def::new("thm", Type::Prop, Term::Global("zero".to_string())).proof(),
    ];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert!(report.state_of("thm").usable_in_proofs());
}

#[test]
fn a_partial_member_makes_its_whole_mutual_group_opaque() {
    let member = |name: &str, other: &str| {
        Def::new(
            name,
            list_to_nat(),
            list_fn("l", call(other, vec![var("l")])),
        )
    };
    let defs = [member("ping", "pong").partial(), member("pong", "ping")];
    let report = analyze(&views(&defs));

    assert!(report.is_clean(), "{:?}", report.failures);
    assert_eq!(report.state_of("pong"), AdmissionState::Partial);
}

#[test]
fn a_rejected_definition_is_neither_reducible_nor_usable() {
    let defs = [spin("spin")];
    let report = analyze(&views(&defs));

    assert!(
        !report.is_clean(),
        "a rejected definition is not a clean report"
    );
    let state = report.state_of("spin");
    assert_eq!(state, AdmissionState::Rejected);
    assert!(!state.is_delta_reducible());
    assert!(!state.usable_in_proofs());
    // An unknown name is not admitted either — the state machine has no
    // "assumed fine" answer.
    assert_eq!(report.state_of("never_seen"), AdmissionState::Rejected);
}

#[test]
fn diagnostics_name_the_group_the_parameter_and_the_argument() {
    let defs = [spin("spin")];
    let failure = &analyze(&views(&defs)).failures[0];

    assert_eq!(failure.headline(), "cannot prove termination of `spin`");
    let notes = failure.notes().join("\n");
    assert!(notes.contains("`l`"), "{notes}");
    assert!(notes.contains("`spin`"), "{notes}");
    assert!(failure.suggestion().unwrap().contains("#[partial]"));
}

#[test]
fn an_ambiguity_suggests_the_annotation_to_write() {
    let inner = super::fixtures::match_list(
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
            list_fn(
                "b",
                super::fixtures::match_list(var("a"), Term::Zero, "t", inner),
            ),
        ),
    )];
    let failure = &analyze(&views(&defs)).failures[0];

    assert_eq!(
        failure.suggestion().unwrap(),
        "annotate the intended parameter, e.g. `#[decreasing(a)]`"
    );
}

#[test]
fn a_mutual_group_is_named_in_the_notes() {
    let member = |name: &str, other: &str| {
        Def::new(
            name,
            list_to_nat(),
            list_fn("l", call(other, vec![var("l")])),
        )
    };
    let defs = [member("ping", "pong"), member("pong", "ping")];
    let failure = &analyze(&views(&defs)).failures[0];

    assert!(failure
        .notes()
        .iter()
        .any(|note| note.contains("mutually recursive group: ping, pong")));
}
