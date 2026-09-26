//! Tests: what the peel *says* when it cannot flatten (ADR 11.8.26c §2.2).
//!
//! The third of the three questions this module answers, after "what does it
//! unfold to" (`tests.rs`) and "does it stop" (`termination.rs`). It is a
//! separate concern because the diagnostic has a job the guard does not: the
//! encoding has *erased* the generic by the time the peel fails, so E0064 has
//! to recover it from the definition — and it must, because
//! `info type type-encoding` is blocked by this very gate and cannot show the
//! reader the chain.

use tungsten_core::Type;

use super::diagnosis::{generic_wrapping, mentions_type};
use super::test_fixtures::{ctor, elaborator_with_rose, make_elaborator, type_def};
use super::{UnflattenedMu, UnflattenedMuCause};
use crate::elaborate::env::TypeDefKind;
use crate::span::Span;

// ============================================================================
// Recovering the wrapping generic from the definition
// ============================================================================

/// The generic the recursive occurrence hides under is recovered from the
/// *definition*, because the encoding has erased it — `μα_Rose. α_Rose`
/// contains no trace of `Wrap`.
#[test]
fn nested_family_diagnostic_names_the_wrapping_generic() {
    let elab = elaborator_with_rose();
    assert_eq!(
        elab.generic_nesting_recursion("Rose"),
        Some("Wrap".to_string())
    );
}

/// An ordinary recursive ADT is nested under nothing, so the diagnostic omits
/// the clause rather than inventing one.
#[test]
fn ordinary_recursion_has_no_wrapping_generic() {
    let mut elab = make_elaborator();
    elab.env.define_type(type_def(
        "Chain",
        TypeDefKind::ADT(vec![
            ctor("End", 0, vec![]),
            ctor("Link", 1, vec![Type::TyVar("Chain".to_string())]),
        ]),
        Some(Type::mu(
            "α_Chain",
            Type::sum(Type::Unit, Type::TyVar("α_Chain".to_string())),
        )),
    ));
    assert_eq!(elab.generic_nesting_recursion("Chain"), None);
}

/// The innermost wrapper is the one reported: for `Wrap<Box<Rose>>` it is
/// `Box` that actually holds the recursive occurrence, and naming the outer
/// one would send the reader to the wrong definition.
#[test]
fn innermost_wrapping_generic_is_the_one_reported() {
    let mut elab = make_elaborator();
    elab.env.define_type(type_def(
        "Deep",
        TypeDefKind::ADT(vec![ctor(
            "D",
            0,
            vec![Type::App(
                "Wrap".to_string(),
                vec![Type::App(
                    "Box".to_string(),
                    vec![Type::TyVar("Deep".to_string())],
                )],
            )],
        )]),
        Some(Type::mu("α_Deep", Type::TyVar("α_Deep".to_string()))),
    ));
    assert_eq!(
        elab.generic_nesting_recursion("Deep"),
        Some("Box".to_string())
    );
}

/// A record's fields are searched too — a nested family can be spelled
/// `type Rose = { kids: Wrap<Rose> }` just as easily as with a constructor.
#[test]
fn a_records_fields_are_searched_for_the_wrapping_generic() {
    let mut elab = make_elaborator();
    elab.env.define_type(type_def(
        "RoseRec",
        TypeDefKind::Record(vec![(
            "kids".to_string(),
            Type::App("Wrap".to_string(), vec![Type::TyVar("RoseRec".to_string())]),
        )]),
        Some(Type::mu("α_RoseRec", Type::TyVar("α_RoseRec".to_string()))),
    ));
    assert_eq!(
        elab.generic_nesting_recursion("RoseRec"),
        Some("Wrap".to_string())
    );
}

/// A type the environment does not know, and one with no searchable body,
/// yield no generic rather than a panic — the diagnostic degrades to its
/// shorter form.
#[test]
fn an_unknown_or_bodiless_type_yields_no_generic() {
    let mut elab = make_elaborator();
    assert_eq!(elab.generic_nesting_recursion("Absent"), None);

    elab.env
        .define_type(type_def("Stubbed", TypeDefKind::Stub, None));
    assert_eq!(elab.generic_nesting_recursion("Stubbed"), None);

    elab.env
        .define_type(type_def("Aliased", TypeDefKind::Alias(Type::Nat), None));
    assert_eq!(elab.generic_nesting_recursion("Aliased"), None);
}

// ============================================================================
// `mentions_type` — the leaf predicate the recovery rests on
// ============================================================================

/// A μ binder is spelled `α_Rose` while the source says `Rose`, so the
/// predicate has to accept both. Asserting only the bare spelling would let
/// the recovery silently stop working the moment the encoder substitutes.
#[test]
fn a_tyvar_matches_under_both_the_bare_and_binder_spellings() {
    assert!(mentions_type(&Type::TyVar("Rose".to_string()), "Rose"));
    assert!(mentions_type(&Type::TyVar("α_Rose".to_string()), "Rose"));
}

/// The negative half, and the one that matters: without it, a predicate that
/// always answered "yes" would pass every test above.
#[test]
fn an_unrelated_tyvar_does_not_match() {
    assert!(!mentions_type(&Type::TyVar("Other".to_string()), "Rose"));
    assert!(!mentions_type(&Type::TyVar("α_Other".to_string()), "Rose"));
    assert!(!mentions_type(&Type::Nat, "Rose"));
    assert!(!mentions_type(&Type::Unit, "Rose"));
}

/// `α_` is a prefix, not a substring: `Rosewood` is a different type.
#[test]
fn a_longer_name_sharing_a_prefix_does_not_match() {
    assert!(!mentions_type(&Type::TyVar("Rosewood".to_string()), "Rose"));
    assert!(!mentions_type(
        &Type::TyVar("α_Rosewood".to_string()),
        "Rose"
    ));
}

/// An `App` matches on its own *name* as well as through its arguments — the
/// two sides of that `||` are independent, so each needs a case where only it
/// can be true.
#[test]
fn an_app_matches_by_name_or_by_argument_independently() {
    // Name matches, argument does not.
    assert!(mentions_type(
        &Type::App("Rose".to_string(), vec![Type::Nat]),
        "Rose"
    ));
    // Argument matches, name does not.
    assert!(mentions_type(
        &Type::App("Wrap".to_string(), vec![Type::TyVar("Rose".to_string())]),
        "Rose"
    ));
    // Neither.
    assert!(!mentions_type(
        &Type::App("Wrap".to_string(), vec![Type::Nat]),
        "Rose"
    ));
}

/// Both sides of a binary type are searched, and each is asserted alone —
/// a predicate that only ever looked left would pass a both-sides fixture.
#[test]
fn both_sides_of_a_binary_type_are_searched() {
    let rose = || Type::TyVar("Rose".to_string());
    for build in [
        Type::product as fn(Type, Type) -> Type,
        Type::sum as fn(Type, Type) -> Type,
    ] {
        assert!(mentions_type(&build(rose(), Type::Nat), "Rose"), "left");
        assert!(mentions_type(&build(Type::Nat, rose()), "Rose"), "right");
        assert!(!mentions_type(&build(Type::Nat, Type::Bool), "Rose"));
    }
    assert!(mentions_type(
        &Type::Arrow(Box::new(rose()), Box::new(Type::Nat)),
        "Rose"
    ));
    assert!(mentions_type(
        &Type::Arrow(Box::new(Type::Nat), Box::new(rose())),
        "Rose"
    ));
}

/// Single-child wrappers are transparent to the search.
#[test]
fn single_child_wrappers_are_searched_through() {
    let rose = Type::TyVar("Rose".to_string());
    assert!(mentions_type(&Type::Ptr(Box::new(rose.clone())), "Rose"));
    assert!(mentions_type(&Type::Ref(Box::new(rose.clone())), "Rose"));
    assert!(mentions_type(&Type::mu("α_X", rose.clone()), "Rose"));
    assert!(mentions_type(
        &Type::Forall("T".to_string(), Box::new(rose)),
        "Rose"
    ));
    assert!(!mentions_type(&Type::Ptr(Box::new(Type::Nat)), "Rose"));
}

/// `generic_wrapping` reports a generic only when the recursion is genuinely
/// *inside* one. A bare recursive occurrence has no wrapper to name.
#[test]
fn a_bare_recursive_occurrence_has_no_wrapping_generic() {
    assert_eq!(
        generic_wrapping(&Type::TyVar("Rose".to_string()), "Rose"),
        None
    );
    assert_eq!(
        generic_wrapping(&Type::App("Wrap".to_string(), vec![Type::Nat]), "Rose"),
        None
    );
}

// ============================================================================
// The rendered E0064 message
// ============================================================================

fn rendered(
    elab: &crate::elaborate::Elaborator<'_>,
    unflattened: &UnflattenedMu,
) -> (String, String) {
    let err = elab.nested_family_error(unflattened, Span::new(0, 0));
    let note = err
        .notes
        .iter()
        .map(|n| n.message.clone())
        .collect::<Vec<_>>()
        .join(" ");
    (err.kind.default_message(), note)
}

/// The message names the **type**, the **binder** and the **generic** — all
/// three, because the tool that would otherwise show them
/// (`info type type-encoding`) cannot run on a file this gate rejects.
#[test]
fn the_e0064_message_names_type_binder_and_generic() {
    let elab = elaborator_with_rose();
    let (message, note) = rendered(
        &elab,
        &UnflattenedMu {
            binder: "α_Rose".to_string(),
            cause: UnflattenedMuCause::BinderRepeats,
        },
    );

    assert!(message.contains("`Rose`"), "names the type: {message}");
    assert!(message.contains("`α_Rose`"), "names the binder: {message}");
    assert!(message.contains("`Wrap`"), "names the generic: {message}");
    assert!(
        note.contains("non-generic intermediate type"),
        "the note carries the workaround: {note}"
    );
    assert!(
        note.contains("`Wrap<Rose>`"),
        "the note names what to replace: {note}"
    );
}

/// With no generic recoverable the nesting clause is dropped rather than
/// rendered empty — and the rest of the message survives.
#[test]
fn the_e0064_message_omits_the_generic_when_there_is_none() {
    let elab = make_elaborator();
    let (message, note) = rendered(
        &elab,
        &UnflattenedMu {
            binder: "α_Ghost".to_string(),
            cause: UnflattenedMuCause::BinderRepeats,
        },
    );

    assert!(message.contains("`Ghost`"), "{message}");
    assert!(message.contains("`α_Ghost`"), "{message}");
    assert!(
        !message.contains("under generic type"),
        "no generic to name, so no dangling clause: {message}"
    );
    assert_eq!(
        note,
        "break the nesting with a non-generic intermediate type"
    );
}

/// A missing cached encoding is a *compiler* inconsistency, and the note says
/// so — telling a user to restructure their types would be a wrong instruction
/// for a bug that is not in their source.
#[test]
fn a_missing_encoding_reads_as_a_compiler_inconsistency() {
    let elab = elaborator_with_rose();
    let (_, note) = rendered(
        &elab,
        &UnflattenedMu {
            binder: "α_Rose".to_string(),
            cause: UnflattenedMuCause::MissingEncoding,
        },
    );

    assert!(note.contains("compiler inconsistency"), "{note}");
    assert!(
        !note.contains("break the nesting"),
        "must not blame the user's types for a compiler bug: {note}"
    );
}

/// Both causes carry the same error code — the distinction is in the note, so
/// a reader who greps for `E0064` finds every instance.
#[test]
fn both_causes_report_e0064() {
    let elab = elaborator_with_rose();
    for cause in [
        UnflattenedMuCause::BinderRepeats,
        UnflattenedMuCause::MissingEncoding,
    ] {
        let err = elab.nested_family_error(
            &UnflattenedMu {
                binder: "α_Rose".to_string(),
                cause,
            },
            Span::new(0, 0),
        );
        assert_eq!(err.kind.code(), "E0064", "{cause:?}");
    }
}
