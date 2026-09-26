//! Tests for `tungsten doctor check comparable`.
//!
//! Everything here drives `analyse_comparability` over in-memory `Type` values
//! — no filesystem, no elaborated project, no captured stdout — except the two
//! CLI entry tests, which exist to pin the exit-code contract.

use std::collections::HashMap;

use tungsten_core::types::Type;

use super::*;
use crate::comparator::ComparatorTypes;
use crate::elaborate::{AdtOrigin, TypeProvenance};
use crate::scratch::ScratchDir;

/// `μα_List. Unit + (Nat × α_List)` — ordinary single-binder recursion, the
/// shape the 100 000-element acceptance criterion runs on.
fn list_of_nat() -> Type {
    Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_List".to_string())),
            )),
        )),
    )
}

/// A two-member cluster in the encoder's nested-binder form: `α_Other` is bound
/// but its body is not carried, so it only resolves through provenance.
fn cluster_member_needing_a_sibling() -> Type {
    Type::Mu(
        "α_Self".to_string(),
        Box::new(Type::Mu(
            "α_Other".to_string(),
            Box::new(Type::Sum(
                Box::new(Type::Unit),
                Box::new(Type::Product(
                    Box::new(Type::TyVar("α_Other".to_string())),
                    Box::new(Type::TyVar("α_Self".to_string())),
                )),
            )),
        )),
    )
}

/// The `ComparatorTypes` a project would supply for the cluster above, with
/// `α_Other` resolving to a self-contained sibling.
fn types_resolving_the_sibling() -> ComparatorTypes {
    let sibling = Type::Mu(
        "α_Other".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_Other".to_string())),
            )),
        )),
    );
    let mut encoded = HashMap::new();
    encoded.insert("Other".to_string(), sibling);
    let mut provenance = TypeProvenance::default();
    provenance.mu_origins.insert(
        "α_Other".to_string(),
        AdtOrigin {
            adt_name: "Other".to_string(),
            type_args: vec![],
            constructors: vec![],
        },
    );
    ComparatorTypes::new(
        crate::driver::RecordTypes::new(),
        &encoded,
        &provenance,
        crate::driver::AdtTypes::new(),
        &HashMap::new(),
    )
}

// ── the happy path ──────────────────────────────────────────────────────────

#[test]
fn a_scalar_is_comparable_and_exits_zero() {
    let report = analyse_comparability(&Type::Nat, &ComparatorTypes::default());
    assert!(report.is_comparable());
    assert_eq!(report.failure, None);
    assert_eq!(report.exit(), ExitCode::SUCCESS);
}

/// The closure must be non-empty on a healthy type. A check whose "proof" arm
/// silently examines nothing would report every type as clean — the vacuity
/// failure this whole check exists to prevent.
#[test]
fn a_healthy_type_actually_synthesizes_comparators() {
    let report = analyse_comparability(
        &Type::Product(Box::new(Type::Nat), Box::new(Type::Bool)),
        &ComparatorTypes::default(),
    );
    assert!(
        report.closure_size > 0,
        "closure was empty — the reference check proved nothing"
    );
}

/// A ≥3-field constructor is ordinary since ADR 1.8.26b D1. This is the
/// removed wide-constructor arm asserted in the *negative*: were the arm still
/// present it would fire here, and the type would be reported broken.
#[test]
fn a_three_field_constructor_is_comparable() {
    let payload = Type::Product(
        Box::new(Type::Nat),
        Box::new(Type::Product(Box::new(Type::String), Box::new(Type::Bool))),
    );
    let ty = Type::Adt(
        "Wide".to_string(),
        vec![],
        vec![
            ("C0".to_string(), Type::Nat),
            ("C1".to_string(), payload),
            ("C2".to_string(), Type::Unit),
        ],
    );
    let report = analyse_comparability(&ty, &ComparatorTypes::default());
    assert!(
        report.is_comparable(),
        "≥3 payload fields is no longer a defect: {:?}",
        report.failure
    );
}

#[test]
fn ordinary_single_binder_recursion_is_comparable() {
    let report = analyse_comparability(&list_of_nat(), &ComparatorTypes::default());
    assert!(
        report.is_comparable(),
        "one binder must not be flagged, or every list type reports broken: {:?}",
        report.failure
    );
}

/// A mutually recursive cluster compares once its members resolve — the
/// removed μ-binder-count arm asserted in the negative.
#[test]
fn a_resolvable_mutual_cluster_is_comparable() {
    let report = analyse_comparability(
        &cluster_member_needing_a_sibling(),
        &types_resolving_the_sibling(),
    );
    assert!(
        report.is_comparable(),
        "multi-binder clusters compare since 1.8.26b D2: {:?}",
        report.failure
    );
}

// ── failure classes ─────────────────────────────────────────────────────────

#[test]
fn a_function_typed_leaf_is_reported_as_opaque_with_its_path() {
    let arrow = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
    let report = analyse_comparability(&arrow, &ComparatorTypes::default());
    let Some(ComparatorFailureKind::OpaqueLeaf { path }) = report.failure else {
        panic!("an Arrow leaf is noncomparable, got {:?}", report.failure);
    };
    assert!(
        path.contains("opaque"),
        "the report must say WHY, not just that it failed: {path}"
    );
}

/// The non-vacuity twin of `a_resolvable_mutual_cluster_is_comparable`: the
/// SAME type with the sibling unresolvable must be rejected, and by name.
/// Without this, a gate that accepted everything would pass that test too.
#[test]
fn an_unresolvable_cluster_member_is_reported_by_the_symbol_it_dangles() {
    let report = analyse_comparability(
        &cluster_member_needing_a_sibling(),
        &ComparatorTypes::default(),
    );
    assert!(
        !report.is_comparable(),
        "a member with no resolvable sibling must not be reported clean"
    );
    assert!(
        matches!(
            report.failure,
            Some(ComparatorFailureKind::IncompleteClosure { .. })
                | Some(ComparatorFailureKind::EmptyClosure)
        ),
        "got {:?}",
        report.failure
    );
}

#[test]
fn the_report_agrees_with_the_gate_by_construction() {
    // One producer, two consumers: the diagnostic must not be able to reach a
    // different verdict from the run. Asserted directly rather than trusted.
    for ty in [
        Type::Nat,
        list_of_nat(),
        Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        cluster_member_needing_a_sibling(),
    ] {
        let report = analyse_comparability(&ty, &ComparatorTypes::default());
        let gate_verdict =
            crate::comparator::gate::classify(&ty, &ComparatorTypes::default()).is_ok();
        assert_eq!(
            report.is_comparable(),
            gate_verdict,
            "diagnostic and enforcement disagree on {ty}"
        );
    }
}

// ── CLI entry: the "cannot look" paths ──────────────────────────────────────
//
// Exit 2 is reserved for bad input. An inability to look is never a finding,
// so neither of these may return 1 — a caller gating on `== 1` would otherwise
// treat a typo'd type name as a comparability defect.

#[test]
fn an_unelaborable_file_is_bad_input_not_a_finding() {
    let scratch = ScratchDir::new("check-comparable-badfile");
    let path = scratch.join("broken.tg");
    std::fs::write(&path, "this is not valid tungsten source {{{").unwrap();
    assert_eq!(
        cmd_check_comparable(Some("Whatever"), &path, false, false, 1),
        ExitCode::from(2)
    );
}

#[test]
fn an_unknown_type_name_is_bad_input_not_a_finding() {
    let scratch = ScratchDir::new("check-comparable-unknown");
    let path = scratch.join("ok.tg");
    std::fs::write(&path, "type Known = | A | B\nfn main() -> Nat { 0 }\n").unwrap();
    assert_eq!(
        cmd_check_comparable(Some("NoSuchType"), &path, false, false, 1),
        ExitCode::from(2)
    );
}

// ── CLI argument resolution (`--all`) ───────────────────────────────────────
//
// Both positionals are `Option` because clap refuses an optional positional
// ahead of a required one, so the arity check is ours to get right. Each
// ambiguous form is pinned, because a wrong resolution silently checks the
// wrong thing rather than failing.

#[test]
fn a_type_and_a_file_resolve_to_a_single_check() {
    assert_eq!(
        resolve_target(Some("Pattern".into()), Some(PathBuf::from("a.tg")), false),
        Ok(Target::One {
            name: "Pattern".into(),
            file: PathBuf::from("a.tg"),
        })
    );
}

#[test]
fn all_with_one_positional_treats_it_as_the_file() {
    // `--all a.tg` puts the file in the FIRST slot, since clap fills
    // positionals in order and the type name is the one being omitted.
    assert_eq!(
        resolve_target(Some("a.tg".into()), None, true),
        Ok(Target::Every {
            file: PathBuf::from("a.tg")
        })
    );
}

#[test]
fn all_with_a_type_and_a_file_is_rejected_rather_than_silently_ignoring_one() {
    // Accepting this would have to discard an argument the user typed, and
    // whichever it discarded would sometimes be the one they meant.
    let err = resolve_target(Some("Pattern".into()), Some(PathBuf::from("a.tg")), true)
        .expect_err("--all plus a type name is contradictory");
    assert!(err.contains("--all"), "{err}");
    assert!(err.contains("drop the type name"), "{err}");
}

#[test]
fn a_lone_positional_without_all_names_the_likely_intent() {
    // The easy mistake: it looks like a file. Clap's bare arity message would
    // say "the following required arguments were not provided", which does not
    // suggest the flag that makes it work.
    let err = resolve_target(Some("a.tg".into()), None, false)
        .expect_err("one positional is not a complete invocation");
    assert!(err.contains("--all <file>"), "{err}");
}

#[test]
fn the_empty_forms_are_rejected() {
    assert!(resolve_target(None, None, true).is_err());
    assert!(resolve_target(None, None, false).is_err());
    assert!(resolve_target(None, Some(PathBuf::from("a.tg")), false).is_err());
}

// ── exit codes (`--all` and the CLI entry) ──────────────────────────────────
//
// Asserted directly, because `ExitCode::default()` is SUCCESS: a body replaced
// wholesale with the default is invisible to any test that only checks output
// text. Mutation found exactly that on `run` and `report_every_type`.

/// A `ProjectOutput` carrying just the stored encodings — the only field these
/// two functions read.
fn project_declaring(types: &[(&str, Type)]) -> ProjectOutput {
    ProjectOutput {
        encoded_types: types
            .iter()
            .map(|(n, t)| ((*n).to_string(), t.clone()))
            .collect(),
        ..Default::default()
    }
}

#[test]
fn all_exits_zero_when_every_type_is_comparable() {
    let project = project_declaring(&[("A", Type::Nat), ("B", Type::Bool)]);
    assert_eq!(
        report_every_type(
            &project,
            &ComparatorTypes::default(),
            &PathBuf::from("x.tg")
        ),
        ExitCode::SUCCESS
    );
}

#[test]
fn all_exits_one_when_any_type_is_not_comparable() {
    // One bad type among good ones must fail the run — a summary that reported
    // the finding and still exited 0 is indistinguishable, to CI, from clean.
    let project = project_declaring(&[
        ("A", Type::Nat),
        ("Bad", Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat))),
    ]);
    assert_eq!(
        report_every_type(
            &project,
            &ComparatorTypes::default(),
            &PathBuf::from("x.tg")
        ),
        ExitCode::from(1)
    );
}

/// An empty corpus is bad input, NOT a clean bill: "all comparable" over
/// nothing is the vacuous-green verdict this whole check exists to prevent.
#[test]
fn all_exits_two_on_a_file_that_declares_no_types() {
    let project = project_declaring(&[]);
    assert_eq!(
        report_every_type(
            &project,
            &ComparatorTypes::default(),
            &PathBuf::from("x.tg")
        ),
        ExitCode::from(2)
    );
}

#[test]
fn the_cli_entry_exits_two_on_every_unusable_argument_form() {
    // `run`'s whole job on these paths is to turn a bad argument set into
    // exit 2 with an explanation; a body that returned SUCCESS would leave a
    // typo looking like a passing check.
    assert_eq!(run(None, None, false, false), ExitCode::from(2));
    assert_eq!(run(None, None, true, false), ExitCode::from(2));
    assert_eq!(
        run(Some("Pattern".into()), None, false, false),
        ExitCode::from(2)
    );
    assert_eq!(
        run(
            Some("Pattern".into()),
            Some(PathBuf::from("a.tg")),
            true,
            false
        ),
        ExitCode::from(2)
    );
}
