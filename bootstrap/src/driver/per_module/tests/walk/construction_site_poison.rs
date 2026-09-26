//! The construction boundaries consume stub poison ACROSS modules (ADR
//! 15.8.26d) — the shape the parent's seeded corpus measured (V3 = 15,
//! V5 = 24).
//!
//! Cross-module is the shape that needs the export half of the fix: the
//! global Signature Collection pass poisons the failed type's stub, and
//! before this ADR that stub was withheld from the exports, so a dependent
//! module kept the Stub Registration placeholder — whose constructor fields
//! are the unresolved source names — and mismatched at every call site.

use super::{elaborate_tree_errors, failing_files};
use crate::elaborate::ElabErrorKind;

/// The seeded fault must be the only failing file.
fn assert_only_the_seeded_file_fails(files: &[(&str, &str)], seeded: &str) {
    let errors = elaborate_tree_errors(files);
    let seeded_reported = errors.iter().any(|e| {
        matches!(&e.kind, ElabErrorKind::UndefinedType(name) if name == "NoSuchType")
            && e.file_path.as_ref().is_some_and(|p| p.ends_with(seeded))
    });
    assert!(
        seeded_reported,
        "the seeded fault must be reported at its own span: {errors:?}"
    );
    let failing = failing_files(&errors);
    assert_eq!(
        failing,
        vec![seeded.to_string()],
        "every other module builds against poison and must stay quiet: {errors:?}"
    );
}

/// The helper's own contract, both halves: a dependent that fails on its own
/// account must fail it, and so must a "seeded" file whose fault is not the
/// seeded kind — the helper asks for BOTH the kind and the file, and a helper
/// that settles for either would pass a cascade that lands in the right file.
#[test]
#[should_panic(expected = "every other module builds against poison and must stay quiet")]
fn the_seeded_file_helper_refuses_a_failing_dependent() {
    assert_only_the_seeded_file_fails(
        &[
            (
                "main.tg",
                "mod events;\nmod emitter;\n\nfn main() -> Nat { 0 }",
            ),
            (
                "events.tg",
                "pub type Event = | Tick(NoSuchType) | Tock(Nat)",
            ),
            (
                "emitter.tg",
                "use events::{Event, Tick};\npub fn tick() -> Event { Tick(undefined_var) }",
            ),
        ],
        "events.tg",
    );
}

#[test]
#[should_panic(expected = "the seeded fault must be reported at its own span")]
fn the_seeded_file_helper_refuses_a_fault_of_the_wrong_kind() {
    assert_only_the_seeded_file_fails(
        &[
            ("main.tg", "mod events;\n\nfn main() -> Nat { 0 }"),
            ("events.tg", "pub fn bad() -> Nat { \"not a nat\" }"),
        ],
        "events.tg",
    );
}

/// The V3 shape: a record whose body fails, CONSTRUCTED and projected from
/// a dependent module. `record_body_fault_reports_once` pins the golden.
#[test]
fn a_seeded_record_body_fault_reports_at_its_span_and_nowhere_else() {
    assert_only_the_seeded_file_fails(
        &[
            (
                "main.tg",
                "mod shapes;\nmod builder;\n\nfn main() -> Nat { 0 }",
            ),
            (
                "shapes.tg",
                "pub type Box = { width: Nat, height: NoSuchType }",
            ),
            (
                "builder.tg",
                "use shapes::{Box};\n\
                 pub fn make() -> Box { Box { width: 1, height: 2 } }\n\
                 pub fn make_anon() -> Box { { width: 1, height: 2 } }\n\
                 pub fn wide(b: Box) -> Nat { b.width }",
            ),
        ],
        "shapes.tg",
    );
}

/// The V5 shape: an ADT whose body fails, its constructors CALLED with
/// arguments and MATCHED from a dependent module. `adt_body_fault_ctor_residue`
/// pins the golden, whose pre-recorded residue this ADR takes to zero.
#[test]
fn a_seeded_adt_body_fault_reports_at_its_span_and_nowhere_else() {
    assert_only_the_seeded_file_fails(
        &[
            (
                "main.tg",
                "mod events;\nmod emitter;\n\nfn main() -> Nat { 0 }",
            ),
            ("events.tg", "pub type Event = | Tick(NoSuchType) | Tock(Nat)"),
            (
                "emitter.tg",
                "use events::{Event, Tick, Tock};\n\
                 pub fn tick() -> Event { Tick(1) }\n\
                 pub fn tock() -> Event { Tock(2) }\n\
                 pub fn is_tick(e: Event) -> Bool { match e { Tick(_) => true, Tock(_) => false } }",
            ),
        ],
        "events.tg",
    );
}

/// The export half in isolation: a dependent's constructor call reaches the
/// poison arm only if the poisoned stub REPLACED the placeholder. With the
/// placeholder still in force the call mismatches against `NoSuchType`
/// (E0010) — the exact residue the parent recorded.
#[test]
fn a_dependent_module_sees_the_poisoned_stub_not_the_placeholder() {
    let errors = elaborate_tree_errors(&[
        (
            "main.tg",
            "mod events;\nmod emitter;\n\nfn main() -> Nat { 0 }",
        ),
        (
            "events.tg",
            "pub type Event = | Tick(NoSuchType) | Tock(Nat)",
        ),
        (
            "emitter.tg",
            "use events::{Event, Tick};\npub fn tick() -> Event { Tick(1) }",
        ),
    ]);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e.kind, ElabErrorKind::TypeMismatch { .. })),
        "an E0010 at the call site means the placeholder, not the poisoned \
         stub, reached the dependent: {errors:?}"
    );
}

/// The Non-Goal, cross-module: an argument's own fault still surfaces in
/// the dependent even though the type it is passed into is poisoned.
#[test]
fn a_dependent_modules_argument_fault_still_surfaces() {
    let errors = elaborate_tree_errors(&[
        (
            "main.tg",
            "mod events;\nmod emitter;\n\nfn main() -> Nat { 0 }",
        ),
        (
            "events.tg",
            "pub type Event = | Tick(NoSuchType) | Tock(Nat)",
        ),
        (
            "emitter.tg",
            "use events::{Event, Tick};\npub fn tick() -> Event { Tick(undefined_var) }",
        ),
    ]);
    let files = failing_files(&errors);
    assert!(
        files.contains(&"emitter.tg".to_string()),
        "the argument's E0001 must survive the transit: {errors:?}"
    );
}
