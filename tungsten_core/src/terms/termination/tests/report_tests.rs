//! How a rejection renders.
//!
//! Every branch of `reason_note` is asserted here, including the ones that only
//! differ by an emptiness guard — a nullary function versus one whose
//! parameters are all the wrong type, and a proof that mentions a partial
//! constant directly versus through a chain. Those pairs read almost the same
//! in the code and say very different things to a reader, which is exactly the
//! kind of difference an only-happy-path suite lets rot.

use crate::terms::termination::{FailureReason, TerminationFailure};

fn failure(reason: FailureReason) -> TerminationFailure {
    TerminationFailure {
        function: "subject".to_string(),
        group: vec!["subject".to_string()],
        span: None,
        reason,
    }
}

/// The single reason note a failure renders.
fn note(reason: FailureReason) -> String {
    let rendered = failure(reason);
    let notes = rendered.notes();
    assert_eq!(notes.len(), 1, "a singleton group adds no group note");
    notes[0].clone()
}

#[test]
fn a_nullary_definition_is_told_it_has_no_parameters_at_all() {
    let rendered = note(FailureReason::NoCandidateParameter {
        parameters: Vec::new(),
    });

    assert_eq!(
        rendered,
        "the definition takes no parameters, so nothing can decrease"
    );
}

#[test]
fn a_definition_whose_parameters_are_all_unsuitable_gets_them_listed() {
    let rendered = note(FailureReason::NoCandidateParameter {
        parameters: vec![
            crate::terms::termination::RejectedRoot {
                name: "n".to_string(),
                rendered_type: "Nat".to_string(),
                because: "a primitive",
            },
            crate::terms::termination::RejectedRoot {
                name: "s".to_string(),
                rendered_type: "String".to_string(),
                because: "a primitive",
            },
        ],
    });

    assert_eq!(
        rendered,
        "no parameter has an inductive type to descend on: `n: Nat` (a primitive), `s: String` (a primitive)"
    );
}

#[test]
fn a_proof_that_mentions_a_partial_constant_directly_names_no_chain() {
    let rendered = note(FailureReason::PartialInProof {
        tainted: "spin".to_string(),
        via: Vec::new(),
    });

    assert_eq!(rendered, "`spin` is marked `#[partial]`");
}

#[test]
fn a_proof_that_reaches_one_through_wrappers_names_the_whole_chain() {
    let rendered = note(FailureReason::PartialInProof {
        tainted: "spin".to_string(),
        via: vec!["outer".to_string(), "inner".to_string()],
    });

    assert_eq!(
        rendered,
        "reaches `spin` (marked `#[partial]`) through outer → inner"
    );
}

#[test]
fn a_headline_says_termination_unless_the_failure_is_the_proof_boundary() {
    assert_eq!(
        failure(FailureReason::PartialInProof {
            tainted: "spin".to_string(),
            via: Vec::new(),
        })
        .headline(),
        "proof `subject` depends on the partial constant `spin`"
    );
    assert_eq!(
        failure(FailureReason::NoCandidateParameter {
            parameters: Vec::new()
        })
        .headline(),
        "cannot prove termination of `subject`"
    );
}

#[test]
fn an_empty_name_list_renders_as_none_rather_than_as_nothing() {
    // `render_list` reached through the annotation branch: an empty parameter
    // list must not print an empty gap where a list belongs.
    let rendered = note(FailureReason::UnknownDecreasingParameter {
        annotated: "nope".to_string(),
        parameters: Vec::new(),
    });
    assert_eq!(rendered, "`nope` is not a parameter of this definition");

    let suggestion = failure(FailureReason::UnknownDecreasingParameter {
        annotated: "nope".to_string(),
        parameters: Vec::new(),
    })
    .suggestion()
    .unwrap();
    assert_eq!(suggestion, "`#[decreasing(…)]` must name a parameter: none");
}

#[test]
fn a_populated_name_list_is_backticked_and_comma_separated() {
    let suggestion = failure(FailureReason::UnknownDecreasingParameter {
        annotated: "nope".to_string(),
        parameters: vec!["a".to_string(), "b".to_string()],
    })
    .suggestion()
    .unwrap();

    assert_eq!(
        suggestion,
        "`#[decreasing(…)]` must name a parameter: `a`, `b`"
    );
}

#[test]
fn an_ambiguity_with_no_candidates_still_suggests_a_usable_annotation() {
    // Defensive: `candidates` is never empty in practice, but the suggestion
    // must stay a valid attribute rather than render `#[decreasing()]`.
    let suggestion = failure(FailureReason::AmbiguousDecreasing {
        candidates: Vec::new(),
    })
    .suggestion()
    .unwrap();

    assert_eq!(
        suggestion,
        "annotate the intended parameter, e.g. `#[decreasing(arg)]`"
    );
}

#[test]
fn an_under_applied_call_names_the_position_it_never_reaches() {
    let rendered = note(FailureReason::PartialApplication {
        callee: "shorted".to_string(),
        position: 2,
    });

    assert_eq!(
        rendered,
        "`shorted` is applied to too few arguments to reach its \
         decreasing parameter (position 2)"
    );
}

#[test]
fn an_unsupported_parameter_type_is_quoted_back() {
    let rendered = note(FailureReason::UnsupportedInductive {
        parameter: "n".to_string(),
        rendered_type: "Nat".to_string(),
    });

    assert_eq!(
        rendered,
        "`n` has type `Nat`, which is not a simple inductive type Phase 1 \
         can descend on"
    );
}
