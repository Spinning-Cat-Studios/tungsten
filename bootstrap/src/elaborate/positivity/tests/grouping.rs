//! Building the engine input from `env.types`, and the SCCs the walker needs.

use tungsten_core::Type;

use crate::elaborate::env::{TypeDef, TypeDefKind};

use super::{adt, alias, record, run, tv};

#[test]
fn every_definition_gets_its_own_group_including_singletons() {
    let report = run(vec![
        adt("A", &[], vec![("MkA", vec![Type::Nat])]),
        adt("B", &[], vec![("MkB", vec![Type::Nat])]),
    ]);
    assert_eq!(report.groups.len(), 2, "{:?}", report.groups);
    assert_eq!(report.max_group_size(), 1);
}

#[test]
fn self_referential_arrow_is_rejected_from_a_size_one_scc() {
    let report = run(vec![adt(
        "Bad",
        &[],
        vec![("Mk", vec![Type::arrow(tv("@Bad"), tv("@Bad"))])],
    )]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(report.max_group_size(), 1, "this is a singleton SCC");
}

#[test]
fn a_record_mediated_cycle_forms_one_group() {
    // The elaborator's own graph is built from `get_adt_types()`, which filters
    // to ADTs — so `R` is not a node there and this cycle is invisible.
    let report = run(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@R"), Type::Nat)])],
        ),
        record("R", vec![("a", tv("@A"))]),
    ]);
    assert_eq!(report.max_group_size(), 2, "{:?}", report.groups);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(report.violations[0].occurrence, "R");
}

#[test]
fn aliases_are_expanded_and_are_not_nodes() {
    let report = run(vec![
        alias("F", &["T"], Type::arrow(tv("T"), Type::Nat)),
        adt(
            "Bad4",
            &[],
            vec![("B", vec![Type::app("F", vec![tv("@Bad4")])])],
        ),
    ]);
    assert!(
        report.groups.iter().all(|g| !g.contains("F")),
        "an alias has no constructor to attribute a violation to: {:?}",
        report.groups
    );
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(report.violations[0].type_name, "Bad4");
    assert_eq!(report.violations[0].ctor_name, "B");
}

#[test]
fn a_deserialized_definition_is_checked_like_any_other() {
    // The import / elab-cache seam: a `TypeDefKind` that arrived by
    // deserialization rather than by collecting source. Taking the test at
    // `deserialized TypeDefKind → env → checker` avoids hand-building
    // elab-cache bytes, which are positional bincode and rot on the next AST
    // variant change.
    let (name, def) = adt(
        "Imported",
        &[],
        vec![("Mk", vec![Type::arrow(tv("@Imported"), Type::Nat)])],
    );
    let encoded = bincode::serialize(&def).expect("TypeDef is Serialize");
    let round_tripped: TypeDef = bincode::deserialize(&encoded).expect("round trip");
    let report = run(vec![(name, round_tripped)]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
}

#[test]
fn a_deserialized_mutual_cycle_is_rejected_in_its_true_scc() {
    // The case a per-import check could not have caught: Import Resolution runs
    // *before* Recursion Grouping, so a per-import walk sees a group of one and
    // false-accepts every mutual violation.
    let pair = vec![
        adt(
            "ImpA",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@ImpB"), Type::Nat)])],
        ),
        adt("ImpB", &[], vec![("MkB", vec![tv("@ImpA")])]),
    ];
    let round_tripped: Vec<(String, TypeDef)> = pair
        .into_iter()
        .map(|(name, def)| {
            let bytes = bincode::serialize(&def).expect("TypeDef is Serialize");
            (name, bincode::deserialize(&bytes).expect("round trip"))
        })
        .collect();
    let report = run(round_tripped);
    assert_eq!(report.max_group_size(), 2, "{:?}", report.groups);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(report.violations[0].occurrence, "ImpB");
}

#[test]
fn a_stub_field_emits_no_violation() {
    // Cross-module stubs lower any complex `TypeExpr` to `Type::Unit`, so
    // doubting one would turn every cross-module reference into a false
    // rejection.
    let report = run(vec![
        (
            "Opaque".to_string(),
            TypeDef::test_stub("Opaque", TypeDefKind::Stub),
        ),
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::app("Opaque", vec![tv("@A")])])],
        ),
    ]);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
    assert!(report.census.stub.contains("Opaque"), "{:?}", report.census);
    assert!(report.census.unknown.is_empty(), "{:?}", report.census);
}

#[test]
fn an_unknown_head_is_censused_as_unknown() {
    let report = run(vec![adt(
        "A",
        &[],
        vec![("MkA", vec![Type::app("Elsewhere", vec![Type::Nat])])],
    )]);
    assert!(
        report.census.unknown.contains("Elsewhere"),
        "{:?}",
        report.census
    );
    assert!(report.census.stub.is_empty(), "{:?}", report.census);
}

#[test]
fn poison_fields_emit_no_violation() {
    let report = run(vec![adt(
        "A",
        &[],
        vec![("MkA", vec![Type::arrow(Type::Error, tv("@A"))])],
    )]);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
}

#[test]
fn the_report_carries_a_span_for_every_definition() {
    let report = run(vec![adt("A", &[], vec![("MkA", vec![Type::Nat])])]);
    // Span comes from the `TypeDef`, so a violation always points at a
    // definition rather than at the file's start by accident.
    assert_eq!(report.span_of("A"), crate::span::Span::default());
    assert_eq!(report.span_of("Absent"), crate::span::Span::default());
}

#[test]
fn tarjan_depth_is_measured() {
    // A → B → C chain: `strongconnect` recurses once per link.
    let report = run(vec![
        adt("A", &[], vec![("MkA", vec![tv("@B")])]),
        adt("B", &[], vec![("MkB", vec![tv("@C")])]),
        adt("C", &[], vec![("MkC", vec![Type::Nat])]),
    ]);
    assert_eq!(report.max_tarjan_depth, 3, "{:?}", report.groups);
}
