//! What the elaborator does with a violation: the E0061 diagnostic, the span
//! it is reported at, the hook, and D3's debug cross-check.

use tungsten_core::Type;

use crate::elaborate::positivity::{groups_are_subsumed, violation_error};

use super::{adt, alias, record, run, tv};

#[test]
fn violation_error_renders_the_inherited_chain() {
    let report = run(vec![
        adt(
            "Fn1",
            &["T"],
            vec![("Mk", vec![Type::arrow(tv("T"), Type::Nat)])],
        ),
        adt(
            "Bad2",
            &[],
            vec![("B", vec![Type::app("Fn1", vec![tv("@Bad2")])])],
        ),
    ]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    let error = violation_error(&report.violations[0], report.span_of("Bad2"));
    assert_eq!(error.kind.code(), "E0061");
    assert_eq!(
        error.message,
        "`Bad2` is not strictly positive: `Bad2` reaches a forbidden position \
         in constructor `B`, field 0 through `Fn1`'s parameter `T`"
    );
    let notes: Vec<&str> = error.notes.iter().map(|n| n.message.as_str()).collect();
    assert!(
        notes
            .iter()
            .any(|n| n.contains("`Fn1` does not use its parameter `T` strictly positively")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("well-founded")),
        "{notes:?}"
    );
}

#[test]
fn violation_error_names_a_record_field() {
    let report = run(vec![
        record("R", vec![("f", Type::arrow(tv("@A"), Type::Nat))]),
        adt("A", &[], vec![("MkA", vec![tv("@R")])]),
    ]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    let error = violation_error(&report.violations[0], report.span_of("R"));
    assert!(
        error.message.contains("record `R`, field `f`"),
        "{}",
        error.message
    );
}

#[test]
fn an_alias_that_erases_its_argument_is_expanded_before_the_walk() {
    // `PhantomAlias` discards its parameter, so inlining it removes the
    // occurrence of `Y` entirely and the type is accepted. Skip the expansion
    // and the alias head is unknown instead, which forbids its arguments and
    // REJECTS a legitimate type — the false-rejection class §5 calls the
    // costlier failure.
    let report = run(vec![
        alias("PhantomAlias", &["T"], Type::Nat),
        adt(
            "Y",
            &[],
            vec![(
                "MkY",
                vec![Type::app(
                    "PhantomAlias",
                    vec![Type::arrow(tv("@Y"), Type::Nat)],
                )],
            )],
        ),
    ]);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
}

#[test]
fn a_violation_is_reported_at_its_definition_span() {
    let (name, mut def) = adt(
        "Bad",
        &[],
        vec![("Mk", vec![Type::arrow(tv("@Bad"), Type::Nat)])],
    );
    def.span = crate::span::Span::new(17, 42);
    let report = run(vec![(name, def)]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    assert_eq!(
        report.span_of("Bad"),
        crate::span::Span::new(17, 42),
        "the diagnostic must point at the definition, not at the file start"
    );
}

#[test]
fn the_elaborator_hook_rejects_a_non_strictly_positive_definition() {
    // The gate itself, through `elaborate_file` — not the engine.
    let errors = crate::elaborate::tests::elab_err("type Bad = Mk(Bad -> Bad)");
    assert!(
        errors.iter().any(|e| e.kind.code() == "E0061"),
        "{errors:?}"
    );
}

#[test]
fn the_elaborator_hook_accepts_a_strictly_positive_definition() {
    // The must-not-fire direction: a guard that fires on healthy input trains
    // its readers to ignore the exit code.
    let errors = crate::elaborate::tests::elab("type Ok = MkOk(Nat -> Ok)\nfn main() -> Nat { 0 }")
        .err()
        .unwrap_or_default();
    assert!(
        !errors.iter().any(|e| e.kind.code() == "E0061"),
        "{errors:?}"
    );
}

#[test]
fn the_debug_cross_check_accepts_a_contained_group_and_rejects_a_split_one() {
    use std::collections::{BTreeSet, HashMap};

    let scc: BTreeSet<String> = ["A", "B"].iter().map(|m| (*m).to_string()).collect();
    let split: Vec<BTreeSet<String>> = vec![
        ["A".to_string()].into_iter().collect(),
        ["B".to_string()].into_iter().collect(),
    ];

    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    groups.insert("A".to_string(), vec!["A".to_string(), "B".to_string()]);

    assert!(
        groups_are_subsumed(groups.values(), &[scc]),
        "our SCC contains the elaborator's group"
    );
    assert!(
        !groups_are_subsumed(groups.values(), &split),
        "a split SCC means our graph is missing an edge — a false accept"
    );
    assert!(
        groups_are_subsumed(std::iter::empty(), &split),
        "no elaborator groups is vacuously subsumed"
    );
}
