//! Tests for the `doctor check type positivity` tally (ADR 7.8.26e §2.4).
//!
//! The tally is the assertable value the command's exit code is derived from —
//! an all-`SUCCESS` exit-code test would assert nothing.

use std::collections::BTreeMap;
use std::process::ExitCode;

use tungsten_core::Type;

use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::positivity::{analyze, from_env_types, PositivityReport};

use super::PositivityTally;

fn report_of(types: Vec<(String, TypeDef)>) -> PositivityReport {
    let map: BTreeMap<String, TypeDef> = types.into_iter().collect();
    let (defs, spans) = from_env_types(map.iter());
    analyze(&defs, spans)
}

fn tally_of(types: Vec<(String, TypeDef)>) -> PositivityTally {
    PositivityTally::from_report(&report_of(types))
}

fn adt(name: &str, ctors: Vec<(&str, Vec<Type>)>) -> (String, TypeDef) {
    adt_generic(name, &[], ctors)
}

fn adt_generic(name: &str, params: &[&str], ctors: Vec<(&str, Vec<Type>)>) -> (String, TypeDef) {
    let mut def = TypeDef::test_stub(
        name,
        TypeDefKind::ADT(
            ctors
                .into_iter()
                .enumerate()
                .map(|(index, (cname, fields))| Constructor::test_with_fields(cname, index, fields))
                .collect(),
        ),
    );
    def.params = params.iter().map(|p| (*p).to_string()).collect();
    (name.to_string(), def)
}

fn tv(name: &str) -> Type {
    Type::TyVar(name.to_string())
}

#[test]
fn a_clean_corpus_tallies_zero_and_exits_success() {
    let tally = tally_of(vec![
        adt("A", vec![("MkA", vec![Type::Nat])]),
        adt("B", vec![("MkB", vec![tv("@A")])]),
    ]);
    assert_eq!(
        tally,
        PositivityTally {
            definitions: 2,
            violating_types: 0,
            violations: 0,
            max_group_size: 1,
            // `A` has no outgoing edges, so `strongconnect` never recurses; `B`
            // reaches an already-indexed `A`. Contrast the mutual pair below,
            // where the recursion is real.
            max_tarjan_depth: 1,
            stub_heads: 0,
            unknown_heads: 0,
        }
    );
    assert_eq!(
        format!("{:?}", tally.exit()),
        format!("{:?}", std::process::ExitCode::SUCCESS)
    );
}

#[test]
fn violations_are_counted_per_type_and_per_field() {
    // One type, two offending fields: V = 1 but violations = 2.
    let tally = tally_of(vec![adt(
        "Bad",
        vec![(
            "Mk",
            vec![
                Type::arrow(tv("@Bad"), Type::Nat),
                Type::arrow(tv("@Bad"), Type::Bool),
            ],
        )],
    )]);
    assert_eq!(tally.violating_types, 1);
    assert_eq!(tally.violations, 2);
    assert_eq!(
        format!("{:?}", tally.exit()),
        format!("{:?}", std::process::ExitCode::FAILURE)
    );
}

#[test]
fn the_census_is_split_by_stub_versus_unknown() {
    let tally = tally_of(vec![
        (
            "Opaque".to_string(),
            TypeDef::test_stub("Opaque", TypeDefKind::Stub),
        ),
        adt(
            "A",
            vec![(
                "MkA",
                vec![
                    Type::app("Opaque", vec![Type::Nat]),
                    Type::app("Elsewhere", vec![Type::Nat]),
                ],
            )],
        ),
    ]);
    assert_eq!(tally.stub_heads, 1);
    assert_eq!(tally.unknown_heads, 1);
    assert_eq!(tally.definitions, 1, "a stub is not a checkable definition");
}

#[test]
fn group_size_and_depth_track_the_expanded_graph() {
    let tally = tally_of(vec![
        adt("A", vec![("MkA", vec![tv("@B")])]),
        adt("B", vec![("MkB", vec![tv("@A")])]),
    ]);
    assert_eq!(tally.max_group_size, 2);
    assert_eq!(tally.max_tarjan_depth, 2);
}

#[test]
fn a_clean_report_renders_the_structural_summary() {
    let report = report_of(vec![
        adt("A", vec![("MkA", vec![Type::Nat])]),
        adt("B", vec![("MkB", vec![tv("@A")])]),
    ]);
    let tally = PositivityTally::from_report(&report);
    let rendered = super::render_report(&report, &tally, false);
    assert_eq!(
        rendered,
        "✓ 2 definition(s) strictly positive (max SCC 1, Tarjan depth 1)\n\
         \x20 unresolved heads: 0 stub, 0 unknown\n"
    );
}

#[test]
fn a_violating_report_names_each_offending_field() {
    let report = report_of(vec![adt(
        "Bad",
        vec![("Mk", vec![Type::arrow(tv("@Bad"), Type::Nat)])],
    )]);
    let tally = PositivityTally::from_report(&report);
    let rendered = super::render_report(&report, &tally, false);
    assert!(rendered.starts_with("✗ 1 type(s) not strictly positive (1 violation(s)):\n"));
    assert!(
        rendered.contains("  Bad.Mk field 0 — `Bad` at a forbidden position\n"),
        "{rendered}"
    );
}

#[test]
fn an_inherited_violation_renders_its_via_chain() {
    let report = report_of(vec![
        adt_generic(
            "Fn1",
            &["T"],
            vec![("Mk", vec![Type::arrow(tv("T"), Type::Nat)])],
        ),
        adt(
            "Bad2",
            vec![("B", vec![Type::app("Fn1", vec![tv("@Bad2")])])],
        ),
    ]);
    let tally = PositivityTally::from_report(&report);
    let rendered = super::render_report(&report, &tally, false);
    assert!(rendered.contains(" via `Fn1`<T>"), "{rendered}");
}

#[test]
fn verbose_adds_parameter_strictness_and_both_census_sections() {
    let report = report_of(vec![
        (
            "Opaque".to_string(),
            TypeDef::test_stub("Opaque", TypeDefKind::Stub),
        ),
        adt(
            "Holder",
            vec![(
                "H",
                vec![
                    Type::app("Opaque", vec![Type::Nat]),
                    Type::app("Elsewhere", vec![Type::Nat]),
                ],
            )],
        ),
    ]);
    let tally = PositivityTally::from_report(&report);

    let terse = super::render_report(&report, &tally, false);
    assert!(!terse.contains("Parameter strictness"), "{terse}");

    let verbose = super::render_report(&report, &tally, true);
    assert!(verbose.contains("\nParameter strictness:\n"), "{verbose}");
    assert!(
        verbose.contains("\nStub heads (skipped): Opaque\n"),
        "{verbose}"
    );
    assert!(
        verbose.contains("\nUnknown heads (doubted): Elsewhere\n"),
        "{verbose}"
    );
}

#[test]
fn verbose_lists_a_parameterized_type_with_its_computed_occurrences() {
    let report = report_of(vec![adt_generic(
        "Pair",
        &["T", "U"],
        vec![("MkPair", vec![tv("T"), Type::arrow(tv("U"), Type::Nat)])],
    )]);
    let tally = PositivityTally::from_report(&report);
    let verbose = super::render_report(&report, &tally, true);
    assert!(verbose.contains("  Pair<strict, forbidden>\n"), "{verbose}");
}

#[test]
fn the_command_exits_nonzero_on_a_file_whose_type_is_not_strictly_positive() {
    // Exercises the whole CLI path — elaborate, `from_project`, analyze, tally,
    // exit — over files written for this test, so the assertions do not depend
    // on the live checkout (the mutation sweep runs in a copied workspace).
    //
    // The violating file exits **2**, not 1: since the gate is hard, a
    // non-strictly-positive file no longer *elaborates*, so the command stops
    // at `elaborate_project` and never reaches its own verdict. Reading the
    // per-type report therefore means fixing the E0061 first — the tool's job
    // is the healthy case (parameter occurrences, SCC sizes, the head census),
    // and `tungsten check` is what explains a rejection.
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad.tg");
    std::fs::write(&bad, "type Bad = Mk(Bad -> Bad)\n").expect("write");
    assert_eq!(
        format!("{:?}", super::cmd_check_positivity(&bad, false, 20)),
        format!("{:?}", ExitCode::from(2))
    );

    let good = dir.path().join("good.tg");
    std::fs::write(&good, "type Ok = MkOk(Nat -> Ok)\n").expect("write");
    assert_eq!(
        format!("{:?}", super::cmd_check_positivity(&good, false, 20)),
        format!("{:?}", ExitCode::SUCCESS)
    );
}

#[test]
fn from_project_carries_the_elaborated_definitions() {
    // `ProjectOutput` is the tool's own input path, distinct from the gate's
    // `env.types`. An empty one would make every report vacuously clean — the
    // "gate that passes having examined less" shape — so assert the definitions
    // and a computed parameter occurrence, not just the verdict.
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("shapes.tg");
    std::fs::write(
        &file,
        "type List<T> = Nil | Cons(T, List<T>)\n\
         type Point = { x: Nat, y: Nat }\n\
         type Count = Nat\n",
    )
    .expect("write");

    let project = crate::driver::elaborate_project(&file, false, 20, None).expect("elaborates");
    let (defs, spans) = crate::elaborate::positivity::from_project(&project);
    let report = crate::elaborate::positivity::analyze(&defs, spans);
    let tally = PositivityTally::from_report(&report);

    assert_eq!(tally.violating_types, 0);
    assert!(
        report.param_occs.contains_key("List") && report.param_occs.contains_key("Point"),
        "ADTs and records must both reach the engine: {:?}",
        report.param_occs.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        report.param_occs["List"],
        vec![tungsten_core::types::positivity::Occ::Strict]
    );
    assert!(
        !report.param_occs.contains_key("Count"),
        "an alias is expanded away, never a node"
    );
}
