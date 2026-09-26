//! The claim §2.2 rests on: reducing is not weakening (ADR 19.8.26d D3).
//!
//! The seam hands the engine only the definitions in a recursive component and
//! carries the rest as name sets. That is a *different call* from the one the
//! bootstrap makes — `analyze_with_carried(everything, empty)` — and the whole
//! design is worthless if the two disagree. So these tests run both over one
//! input and compare the verdicts.
//!
//! Comparing rejection *sets* rather than counts is deliberate: two analyses
//! that reject the same number of definitions for different reasons agree on
//! nothing that matters.

use std::collections::{BTreeMap, BTreeSet};

use super::super::registry::{ItemNote, TerminationRegistry};
use crate::terms::termination::{
    analyze_with_carried, CarriedDefs, DefRole, DefView, TerminationAnnotation,
};
use crate::terms::Term;
use crate::types::Type;

/// One definition, owned so the borrowed views can point at it.
struct Def {
    name: String,
    ty: Type,
    term: Term,
    annotation: TerminationAnnotation,
    role: DefRole,
}

fn def(name: &str, term: Term) -> Def {
    Def {
        name: name.to_string(),
        ty: Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        term,
        annotation: TerminationAnnotation::default(),
        role: DefRole::Executable,
    }
}

fn partial(mut d: Def) -> Def {
    d.annotation.partial = true;
    d
}

fn proof(mut d: Def) -> Def {
    d.role = DefRole::Proof;
    d
}

/// `f(0)` — a call that supplies an argument, so it is a graph edge.
fn calls(name: &str) -> Term {
    Term::App(
        Box::new(Term::Global(name.to_string())),
        Box::new(Term::Zero),
    )
}

fn view(d: &Def) -> DefView<'_> {
    DefView {
        ty: &d.ty,
        term: &d.term,
        annotation: &d.annotation,
        role: d.role,
    }
}

/// What the bootstrap does: every definition, nothing carried.
fn whole_project(defs: &[Def]) -> BTreeSet<String> {
    let views: BTreeMap<String, DefView<'_>> =
        defs.iter().map(|d| (d.name.clone(), view(d))).collect();
    rejections(&analyze_with_carried(&views, &CarriedDefs::default()))
}

/// What the seam does, driven through the **real registry** rather than a
/// re-statement of it.
///
/// Driving `TerminationRegistry` itself is the point: a copy of the retention
/// rule written here would keep agreeing with itself after the rule in
/// `registry.rs` changed, which is the one failure this test exists to catch.
/// The registry's payload arguments are already-owned `Type`/`Term`, so no
/// arena is needed and these stay pure-function tests.
fn reduced(defs: &[Def]) -> BTreeSet<String> {
    let mut registry = TerminationRegistry::default();
    for d in defs {
        registry
            .note_item(
                &d.name,
                ItemNote {
                    annotation: d.annotation.clone(),
                    proof: d.role == DefRole::Proof,
                },
            )
            .expect("note in the reduce phase");
        registry.declare(&d.name).expect("declare before reducing");
    }
    for d in defs {
        registry
            .add_def(&d.name, d.ty.clone(), d.term.clone())
            .expect("reduce");
    }
    registry.plan(true).expect("plan");
    for d in defs {
        registry
            .add_def(&d.name, d.ty.clone(), d.term.clone())
            .expect("retain");
    }
    registry.check().expect("check");
    registry
        .failures()
        .iter()
        .map(|f| format!("{}: {}", f.function, f.headline()))
        .collect()
}

/// Who was rejected, and for what — the comparison that means something.
fn rejections(report: &crate::terms::termination::TerminationReport) -> BTreeSet<String> {
    report
        .failures
        .iter()
        .map(|f| format!("{}: {}", f.function, f.headline()))
        .collect()
}

/// Both analyses over one input, asserted equal and asserted non-empty where
/// the case is a rejecting one.
fn assert_agrees(defs: &[Def]) -> BTreeSet<String> {
    let whole = whole_project(defs);
    let split = reduced(defs);
    assert_eq!(whole, split, "the reduced seam changed the verdict");
    whole
}

#[test]
fn a_non_descending_self_call_is_rejected_by_both() {
    let found = assert_agrees(&[def("loop", calls("loop"))]);
    assert_eq!(found.len(), 1, "and the case is a rejecting one");
}

#[test]
fn a_mutual_cycle_that_makes_no_progress_is_rejected_by_both() {
    let found = assert_agrees(&[def("even", calls("odd")), def("odd", calls("even"))]);
    assert!(!found.is_empty());
}

#[test]
fn a_proof_reaching_a_partial_constant_is_rejected_by_both() {
    let found = assert_agrees(&[
        partial(def("looper", calls("looper"))),
        proof(def("thm", calls("looper"))),
    ]);
    assert!(
        found.iter().any(|f| f.starts_with("thm:")),
        "the proof boundary is the rejection, not the annotated definition: {found:?}"
    );
}

#[test]
fn taint_through_a_definition_the_plan_drops_still_reaches_the_proof() {
    // `wrapper` is neither recursive nor annotated, so the plan drops it and it
    // exists for the engine only as a carried name set. It is the *only* path
    // from the proof to the partial constant.
    let found = assert_agrees(&[
        partial(def("looper", calls("looper"))),
        def("wrapper", calls("looper")),
        proof(def("thm", calls("wrapper"))),
    ]);
    assert!(found.iter().any(|f| f.starts_with("thm:")), "{found:?}");
}

#[test]
fn a_partial_member_taints_its_non_annotated_group_siblings() {
    // The arm that decides the retention rule: this group is skipped by
    // descent, so a plan retaining only *checked* groups would drop it — and
    // `sibling`, which is not itself annotated, would stop seeding taint.
    let found = assert_agrees(&[
        partial(def("a", calls("b"))),
        def("b", calls("a")),
        proof(def("thm", calls("b"))),
    ]);
    assert!(found.iter().any(|f| f.starts_with("thm:")), "{found:?}");
}

#[test]
fn a_clean_corpus_is_accepted_by_both() {
    let found = assert_agrees(&[
        def("leaf", Term::Zero),
        def("caller", calls("leaf")),
        proof(def("thm", calls("caller"))),
    ]);
    assert!(found.is_empty(), "nothing here recurses: {found:?}");
}

#[test]
fn the_two_analyses_are_distinguishable_when_the_carried_set_is_wrong() {
    // Non-vacuity for the comparison itself: drop the carried channel and the
    // reduced side stops seeing the taint path, so `assert_agrees` would have
    // to fail. If this test ever passes with an EMPTY difference, the
    // comparison above is asserting nothing.
    let defs = [
        partial(def("looper", calls("looper"))),
        def("wrapper", calls("looper")),
        proof(def("thm", calls("wrapper"))),
    ];
    let views: BTreeMap<String, DefView<'_>> = defs
        .iter()
        .filter(|d| d.name == "looper")
        .map(|d| (d.name.clone(), view(d)))
        .collect();
    let without_carried = rejections(&analyze_with_carried(&views, &CarriedDefs::default()));
    assert_ne!(
        without_carried,
        whole_project(&defs),
        "losing the carried set must change the verdict, or the seam proves nothing"
    );
}
