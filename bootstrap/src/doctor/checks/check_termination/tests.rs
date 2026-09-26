//! Tests for the termination report's tally and rendering.
//!
//! The reports are built by hand rather than by elaborating a file: the tally
//! and the rendering are what this module owns, and a fixture that had to
//! elaborate would test the engine again instead.

use std::collections::BTreeMap;

use tungsten_core::terms::termination::{
    AdmissionState, FailureReason, TerminationFailure, TerminationReport,
};

use super::{render_report, TerminationTally};

fn report_with(
    states: &[(&str, AdmissionState)],
    failures: Vec<TerminationFailure>,
) -> TerminationReport {
    TerminationReport {
        admission: states
            .iter()
            .map(|(name, state)| ((*name).to_string(), *state))
            .collect::<BTreeMap<_, _>>(),
        recursive_groups: vec![vec!["spin".to_string()]],
        tainted: Vec::new(),
        failures,
    }
}

fn no_descent(function: &str) -> TerminationFailure {
    TerminationFailure {
        function: function.to_string(),
        group: vec![function.to_string()],
        span: None,
        reason: FailureReason::NoDescent {
            parameter: "l".to_string(),
            callee: function.to_string(),
            argument: "l".to_string(),
        },
    }
}

fn partial_in_proof(function: &str) -> TerminationFailure {
    TerminationFailure {
        function: function.to_string(),
        group: Vec::new(),
        span: None,
        reason: FailureReason::PartialInProof {
            tainted: "spin".to_string(),
            via: Vec::new(),
        },
    }
}

#[test]
fn a_clean_report_counts_admissions_and_exits_zero() {
    let report = report_with(
        &[
            ("len", AdmissionState::Total),
            ("spin", AdmissionState::Partial),
        ],
        Vec::new(),
    );
    let tally = TerminationTally::from_report(&report);

    assert_eq!(tally.definitions, 2);
    assert_eq!(tally.total, 1);
    assert_eq!(tally.partial, 1);
    assert_eq!(tally.rejected, 0);
    assert_eq!(tally.recursive_groups, 1);
    assert_eq!(
        format!("{:?}", tally.exit()),
        format!("{:?}", std::process::ExitCode::SUCCESS)
    );

    let text = render_report(&report, &tally, false);
    assert!(text.contains("✓ 2 definition(s) admitted"), "{text}");
    assert!(text.contains("1 total, 1 partial, 0 rejected"), "{text}");
}

#[test]
fn a_rejection_is_listed_with_its_reason_and_exits_non_zero() {
    let report = report_with(
        &[("spin", AdmissionState::Rejected)],
        vec![no_descent("spin")],
    );
    let tally = TerminationTally::from_report(&report);

    assert_eq!(tally.rejected, 1);
    assert_eq!(tally.total, 0);
    assert_ne!(
        format!("{:?}", tally.exit()),
        format!("{:?}", std::process::ExitCode::SUCCESS)
    );

    let text = render_report(&report, &tally, false);
    assert!(
        text.contains("✗ 1 definition(s) not admitted of 1"),
        "{text}"
    );
    assert!(
        text.contains("cannot prove termination of `spin`"),
        "{text}"
    );
    assert!(text.contains("not a known strict subterm"), "{text}");
}

#[test]
fn each_admission_state_is_counted_under_its_own_heading() {
    // Asymmetric on purpose: with one of each state, swapping any two of the
    // three predicates would produce the same three numbers.
    let report = report_with(
        &[
            ("a", AdmissionState::Total),
            ("b", AdmissionState::Total),
            ("c", AdmissionState::Total),
            ("d", AdmissionState::Partial),
            ("e", AdmissionState::Partial),
            ("f", AdmissionState::Rejected),
        ],
        vec![no_descent("f")],
    );
    let tally = TerminationTally::from_report(&report);

    assert_eq!(tally.definitions, 6);
    assert_eq!(tally.total, 3);
    assert_eq!(tally.partial, 2);
    assert_eq!(tally.rejected, 1);
}

#[test]
fn a_proof_failure_alone_still_exits_non_zero() {
    // The proof is not itself rejected — its dependency is partial — so the
    // rejected count is zero and only the proof-failure count is not.
    let report = report_with(
        &[
            ("thm", AdmissionState::Partial),
            ("spin", AdmissionState::Partial),
        ],
        vec![partial_in_proof("thm")],
    );
    let tally = TerminationTally::from_report(&report);

    assert_eq!(tally.rejected, 0);
    assert_eq!(tally.proof_failures, 1);
    assert_ne!(
        format!("{:?}", tally.exit()),
        format!("{:?}", std::process::ExitCode::SUCCESS)
    );
}

#[test]
fn verbose_adds_the_group_listing_and_the_taint_census() {
    let mut report = report_with(&[("spin", AdmissionState::Partial)], Vec::new());
    report.tainted = vec!["spin".to_string(), "wrapper".to_string()];

    let tally = TerminationTally::from_report(&report);
    let quiet = render_report(&report, &tally, false);
    let loud = render_report(&report, &tally, true);

    assert!(!quiet.contains("Recursive groups:"), "{quiet}");
    assert!(loud.contains("Recursive groups:\n  spin"), "{loud}");
    assert!(loud.contains("Tainted: spin, wrapper"), "{loud}");
}

#[test]
fn an_empty_taint_census_is_omitted_rather_than_printed_blank() {
    let report = report_with(&[("len", AdmissionState::Total)], Vec::new());
    let tally = TerminationTally::from_report(&report);

    assert!(!render_report(&report, &tally, true).contains("Tainted:"));
}
