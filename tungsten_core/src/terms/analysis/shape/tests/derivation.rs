//! What a node's type is, when nothing recorded it outright (ADR 3.9.26h AC3).
//!
//! The sibling file asserts the rule table over `let`-bound operands, which is
//! the one place a type is written down. These assert the derivation the walk
//! runs everywhere else: a wrong one invents a finding or hides a real one, and
//! neither is visible in the table.

use super::{bound, found, pair_type, var, Eliminator, Former};
use crate::terms::{Term, TermSpan};
use crate::types::Type;

#[test]
fn a_fix_reports_its_annotation_rather_than_its_bodys_type() {
    let over_the_annotation = Term::Fst(Box::new(Term::Fix(
        "f".to_string(),
        pair_type(),
        Box::new(Term::Sorry),
    )));
    assert_eq!(found(&over_the_annotation), vec![]);

    // The body is still walked, so a fault inside a fixpoint is still named.
    let fault_inside = Term::Fix(
        "f".to_string(),
        Type::Nat,
        Box::new(bound(Type::Nat, Term::Fst(var()))),
    );
    assert_eq!(
        found(&fault_inside),
        vec![(Eliminator::Fst, Former::Ground("Nat"))]
    );
}

#[test]
fn a_pair_carries_both_sides_and_projecting_past_them_runs_out() {
    let pair = Term::Pair(
        Box::new(bound(Type::Nat, *var())),
        Box::new(bound(Type::String, *var())),
    );
    assert_eq!(found(&Term::Fst(Box::new(pair.clone()))), vec![]);
    assert_eq!(
        found(&Term::Fst(Box::new(Term::Fst(Box::new(pair))))),
        vec![(Eliminator::Fst, Former::Ground("Nat"))]
    );
}

#[test]
fn a_ref_cell_is_not_its_contents_until_it_is_read() {
    let cell = Term::RefNew(Box::new(bound(pair_type(), *var())));
    assert_eq!(
        found(&Term::Fst(Box::new(cell.clone()))),
        vec![(Eliminator::Fst, Former::Ref)]
    );
    let read = Term::RefGet(Box::new(cell));
    assert_eq!(found(&Term::Fst(Box::new(read))), vec![]);

    // Reading something that is not a cell derives nothing — silence, not a
    // finding, which is the rule the whole check is budgeted around.
    let read_a_scalar = Term::RefGet(Box::new(bound(Type::Nat, *var())));
    assert_eq!(found(&Term::Fst(Box::new(read_a_scalar))), vec![]);
}

#[test]
fn a_type_abstraction_is_a_forall_and_a_span_is_transparent() {
    let abstraction = Term::TyAbs("A".to_string(), Box::new(bound(Type::Nat, *var())));
    assert_eq!(
        found(&Term::Fst(Box::new(abstraction))),
        vec![(Eliminator::Fst, Former::Forall)]
    );

    let spanned = Term::Spanned(Box::new(bound(Type::Nat, *var())), TermSpan::new(0, 1));
    assert_eq!(
        found(&Term::Snd(Box::new(spanned))),
        vec![(Eliminator::Snd, Former::Ground("Nat"))]
    );
}

#[test]
fn every_annotated_form_reports_the_type_it_was_annotated_with() {
    // These are the forms whose own annotation IS their type. Each is put under
    // a `fst`, which no non-product admits, so the finding names the annotation
    // the derivation read — silence here would mean the annotation was dropped.
    let annotated = [
        Term::Absurd(Type::Nat, Box::new(Term::Sorry)),
        Term::Inl(Type::Nat, Box::new(Term::Sorry)),
        Term::Inr(Type::Nat, Box::new(Term::Sorry)),
        Term::Annot(Box::new(Term::Sorry), Type::Nat),
        Term::AdtConstruct(Type::Nat, 0, Box::new(Term::Sorry)),
        Term::NatRec(
            Type::Nat,
            Box::new(Term::Sorry),
            Box::new(Term::Sorry),
            Box::new(Term::Sorry),
        ),
        Term::NatInd(
            Type::Nat,
            Box::new(Term::Sorry),
            Box::new(Term::Sorry),
            Box::new(Term::Sorry),
        ),
    ];
    for term in annotated {
        assert_eq!(
            found(&Term::Fst(Box::new(term.clone()))),
            vec![(Eliminator::Fst, Former::Ground("Nat"))],
            "{term:?}"
        );
    }
}

#[test]
fn a_refl_witness_is_an_equality_and_no_projection_admits_one() {
    let proof = Term::Refl(Type::Nat, Box::new(Term::Zero));
    assert_eq!(
        found(&Term::Fst(Box::new(proof))),
        vec![(Eliminator::Fst, Former::Equality)]
    );
}

#[test]
fn a_case_binds_each_arm_at_its_own_side_of_the_sum() {
    // `sum_sides` is what gives an arm variable a type at all. Dropping both
    // sides leaves every arm unrecorded, which is silence — so a walk that had
    // stopped reading the scrutinee would pass every other test in this file.
    let scrutinee = bound(
        Type::Sum(Box::new(Type::Nat), Box::new(pair_type())),
        *var(),
    );
    let term = Term::Case(
        Box::new(scrutinee),
        "l".to_string(),
        Box::new(Term::Fst(Box::new(Term::Var("l".to_string())))),
        "r".to_string(),
        Box::new(Term::Snd(Box::new(Term::Var("r".to_string())))),
    );
    // Left is a `Nat`, so its `fst` has nothing to project; right is the pair,
    // so its `snd` stands — one finding, not none and not two.
    assert_eq!(found(&term), vec![(Eliminator::Fst, Former::Ground("Nat"))]);
}

#[test]
fn a_match_whose_scrutinee_records_no_variants_binds_nothing() {
    let arm = Term::Fst(Box::new(Term::Var("payload".to_string())));
    let term = Term::AdtMatch(
        Box::new(bound(Type::Nat, *var())),
        vec![(0, "payload".to_string(), Box::new(arm))],
    );
    assert_eq!(found(&term), vec![]);
}

#[test]
fn every_former_and_every_eliminator_reads_as_itself() {
    // Both label tables, asserted as tables. A description that fell through to
    // a neighbour's would make a finding name the wrong shape, and the census
    // is read by eye — nothing downstream would catch it.
    let formers = [
        Former::Arrow,
        Former::Product,
        Former::Sum,
        Former::Mu,
        Former::Forall,
        Former::Adt,
        Former::Ref,
        Former::Ptr,
        Former::Equality,
        Former::Ground("Nat"),
        Former::Opaque,
    ];
    let descriptions: Vec<&str> = formers.iter().map(|f| f.description()).collect();
    let mut distinct = descriptions.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), formers.len(), "{descriptions:?}");

    let names: Vec<&str> = [
        Eliminator::Fst,
        Eliminator::Snd,
        Eliminator::App,
        Eliminator::Case,
        Eliminator::Unfold,
    ]
    .iter()
    .map(|e| e.name())
    .collect();
    assert_eq!(names, ["fst", "snd", "app", "case", "unfold"]);
}
