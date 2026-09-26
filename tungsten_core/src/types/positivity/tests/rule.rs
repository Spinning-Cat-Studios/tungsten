//! What the walker decides about an OCCURRENCE: which positions are
//! forbidden, how the `@`-prefix is resolved, and the accepted side of the
//! gate. The parameter-strictness half of D2 lives in [`super::parameters`].

use crate::types::Type;

use super::{adt, env, group, record, tv, violations};
use crate::types::positivity::*;
// The headline witness: a size-1 SCC
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn self_reference_left_of_arrow_is_rejected() {
    // type Bad = Mk(Bad -> Bad)
    let defs = env(vec![adt(
        "Bad",
        &[],
        vec![("Mk", vec![Type::arrow(tv("@Bad"), tv("@Bad"))])],
    )]);
    let found = violations(&defs, &["Bad"]);
    assert_eq!(found.len(), 1, "expected exactly one violation: {found:?}");
    assert_eq!(found[0].type_name, "Bad");
    assert_eq!(found[0].ctor_name, "Mk");
    assert_eq!(found[0].occurrence, "Bad");
    assert_eq!(found[0].field, FieldRef::Index(0));
    assert!(
        found[0].via.is_empty(),
        "direct occurrence has no via chain"
    );
}

#[test]
fn codomain_occurrence_alone_is_accepted() {
    // type Ok = Mk(Nat -> Ok)
    let defs = env(vec![adt(
        "Ok",
        &[],
        vec![("Mk", vec![Type::arrow(Type::Nat, tv("@Ok"))])],
    )]);
    assert!(violations(&defs, &["Ok"]).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// D2: strict, not merely positive
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn twice_left_of_arrow_is_still_rejected() {
    // type Bad3 = Mk((Bad3 -> Nat) -> Nat) — positive, but not strictly.
    // A sign lattice with Negative ⊗ Negative = Positive accepts this.
    let defs = env(vec![adt(
        "Bad3",
        &[],
        vec![(
            "Mk",
            vec![Type::arrow(Type::arrow(tv("@Bad3"), Type::Nat), Type::Nat)],
        )],
    )]);
    let found = violations(&defs, &["Bad3"]);
    assert_eq!(
        found.len(),
        1,
        "double-negative must not flip back: {found:?}"
    );
    assert_eq!(found[0].occurrence, "Bad3");
}

#[test]
fn forbidden_mode_is_absorbing() {
    assert_eq!(Mode::Forbidden.descend(Occ::Strict), Some(Mode::Forbidden));
    assert_eq!(Mode::Strict.descend(Occ::Strict), Some(Mode::Strict));
    assert_eq!(
        Mode::Strict.descend(Occ::Forbidden),
        Some(Mode::Forbidden),
        "a forbidden parameter escalates a strict walk"
    );
    assert_eq!(Mode::Strict.descend(Occ::Unused), None);
    assert_eq!(Mode::Forbidden.descend(Occ::Unused), None);
}

#[test]
fn occ_join_is_the_least_upper_bound() {
    assert_eq!(Occ::Unused.join(Occ::Strict), Occ::Strict);
    assert_eq!(Occ::Strict.join(Occ::Unused), Occ::Strict);
    assert_eq!(Occ::Strict.join(Occ::Forbidden), Occ::Forbidden);
    assert_eq!(Occ::Forbidden.join(Occ::Strict), Occ::Forbidden);
    assert_eq!(Occ::Unused.join(Occ::Unused), Occ::Unused);
}

// ─────────────────────────────────────────────────────────────────────────
// D3: mutual and record-mediated cycles
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mutual_violation_is_rejected_and_the_arrow_free_pair_accepted() {
    // type A = MkA(B -> Nat) + type B = MkB(A)
    let bad = env(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@B"), Type::Nat)])],
        ),
        adt("B", &[], vec![("MkB", vec![tv("@A")])]),
    ]);
    let found = violations(&bad, &["A", "B"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].occurrence, "B");

    let good = env(vec![
        adt("A", &[], vec![("MkA", vec![tv("@B")])]),
        adt("B", &[], vec![("MkB", vec![tv("@A")])]),
    ]);
    assert!(violations(&good, &["A", "B"]).is_empty());
}

#[test]
fn record_mediated_cycle_is_a_graph_edge() {
    // type A = MkA(R -> Nat) + type R = { a: A }
    // R is not an ADT, so the elaborator's ADT-only graph cannot see this cycle.
    let defs = env(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@R"), Type::Nat)])],
        ),
        record("R", vec![("a", tv("@A"))]),
    ]);
    let edges = referenced_names(&defs);
    assert!(edges["A"].contains("R"), "{edges:?}");
    assert!(edges["R"].contains("A"), "{edges:?}");

    let found = violations(&defs, &["A", "R"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].occurrence, "R");
}

#[test]
fn record_violation_reports_a_named_field() {
    // type R = { f: A -> Nat } with A = MkA(R): the violation lands on R's field.
    let defs = env(vec![
        record("R", vec![("f", Type::arrow(tv("@A"), Type::Nat))]),
        adt("A", &[], vec![("MkA", vec![tv("@R")])]),
    ]);
    let found = violations(&defs, &["A", "R"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].field, FieldRef::Named("f".to_string()));
    assert!(found[0].is_record);
    assert_eq!(
        found[0].ctor_name, "R",
        "records use an implicit constructor"
    );
}

// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// Determinism and dedup
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn duplicate_occurrences_are_reported_once() {
    // type Bad = Mk(Bad -> Bad) already covers dedup within one field; here the
    // same forbidden occurrence appears twice in the same domain.
    let defs = env(vec![adt(
        "Bad",
        &[],
        vec![(
            "Mk",
            vec![Type::arrow(
                Type::product(tv("@Bad"), tv("@Bad")),
                Type::Nat,
            )],
        )],
    )]);
    assert_eq!(violations(&defs, &["Bad"]).len(), 1);
}

#[test]
fn distinct_fields_are_reported_separately() {
    let defs = env(vec![adt(
        "Bad",
        &[],
        vec![(
            "Mk",
            vec![
                Type::arrow(tv("@Bad"), Type::Nat),
                Type::arrow(tv("@Bad"), Type::Bool),
            ],
        )],
    )]);
    let found = violations(&defs, &["Bad"]);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].field, FieldRef::Index(0));
    assert_eq!(found[1].field, FieldRef::Index(1));
}

#[test]
fn field_ref_renders_positionally_and_by_name() {
    assert_eq!(FieldRef::Index(2).to_string(), "field 2");
    assert_eq!(FieldRef::Named("a".into()).to_string(), "field `a`");
}

#[test]
fn occ_labels_are_stable() {
    assert_eq!(Occ::Unused.label(), "unused");
    assert_eq!(Occ::Strict.label(), "strict");
    assert_eq!(Occ::Forbidden.label(), "forbidden");
}
