//! Tests for the gate adapter: metadata conversion, error rendering and the
//! enforcement split. The cache carry-forward half is in `cache_tests`.

mod cache_tests;

use std::collections::HashMap;

use tungsten_core::terms::{SpannedTerm, Term, TermSpan};
use tungsten_core::types::Type;

use crate::ast::{Ident, TerminationAttrs};
use crate::elaborate::{CoreDef, DefTerminationMeta};
use crate::span::Span;

use super::{CachedTermination, Enforcement, TerminationInput};

fn def(name: &str, ty: Type, term: Term) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty,
        term: SpannedTerm::generated(term),
        span: Span::new(10, 20),
    }
}

fn list_type() -> Type {
    Type::Mu(
        "α".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::TyVar("α".to_string())),
        )),
    )
}

/// `fn spin(l) { spin(l) }` — recursive, never decreasing.
fn spin(name: &str) -> CoreDef {
    def(
        name,
        Type::Arrow(Box::new(list_type()), Box::new(Type::Nat)),
        Term::Lambda(
            "l".to_string(),
            list_type(),
            Box::new(Term::App(
                Box::new(Term::Global(name.to_string())),
                Box::new(Term::Var("l".to_string())),
            )),
        ),
    )
}

fn meta(entries: &[(&str, DefTerminationMeta)]) -> HashMap<String, DefTerminationMeta> {
    entries
        .iter()
        .map(|(name, entry)| ((*name).to_string(), entry.clone()))
        .collect()
}

fn partial_meta() -> DefTerminationMeta {
    DefTerminationMeta {
        attrs: TerminationAttrs {
            partial: true,
            decreasing: None,
        },
        is_proof: false,
    }
}

fn proof_meta() -> DefTerminationMeta {
    DefTerminationMeta {
        attrs: TerminationAttrs::default(),
        is_proof: true,
    }
}

#[test]
fn annotations_survive_the_conversion_from_elaborator_metadata() {
    let entries = meta(&[
        ("spin", partial_meta()),
        (
            "keep",
            DefTerminationMeta {
                attrs: TerminationAttrs {
                    partial: false,
                    decreasing: Some(Ident::new("b", Span::new(0, 1))),
                },
                is_proof: false,
            },
        ),
        ("thm", proof_meta()),
    ]);
    let input = TerminationInput::from_meta(&entries);

    assert!(input.is_proof("thm"));
    assert!(!input.is_proof("spin"));
    assert!(!input.is_proof("never_recorded"));
    // `spin` opts out, so it is admitted rather than rejected.
    assert!(input.check(&[spin("spin")]).is_clean());
}

#[test]
fn an_unannotated_definition_is_checked_and_rejected() {
    let report = TerminationInput::from_meta(&HashMap::new()).check(&[spin("spin")]);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].function, "spin");
}

#[test]
fn a_rejection_without_a_call_site_span_falls_back_to_the_definition() {
    let defs = [spin("spin")];
    let report = TerminationInput::from_meta(&HashMap::new()).check(&defs);
    let errors = super::admission_errors(&report, &defs);

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind.code(), "E0062");
    assert_eq!(errors[0].span, Span::new(10, 20));
    assert!(errors[0].help.is_some());
    assert!(!errors[0].notes.is_empty());
}

#[test]
fn a_call_site_span_is_preferred_over_the_definition_span() {
    let mut spinner = spin("spin");
    spinner.term = SpannedTerm::generated(Term::Lambda(
        "l".to_string(),
        list_type(),
        Box::new(Term::Spanned(
            Box::new(Term::App(
                Box::new(Term::Global("spin".to_string())),
                Box::new(Term::Var("l".to_string())),
            )),
            TermSpan::new(44, 55),
        )),
    ));
    let defs = [spinner];
    let report = TerminationInput::from_meta(&HashMap::new()).check(&defs);

    assert_eq!(
        super::admission_errors(&report, &defs)[0].span,
        Span::new(44, 55)
    );
}

#[test]
fn a_proof_reaching_a_partial_constant_renders_as_e0063() {
    let defs = [
        spin("spin"),
        def(
            "thm",
            Type::Prop,
            Term::App(
                Box::new(Term::Global("spin".to_string())),
                Box::new(Term::Unit),
            ),
        ),
    ];
    let entries = meta(&[("spin", partial_meta()), ("thm", proof_meta())]);
    let report = TerminationInput::from_meta(&entries).check(&defs);

    let errors = super::admission_errors(&report, &defs);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind.code(), "E0063");
    assert!(
        errors[0].message.contains("`spin`"),
        "{}",
        errors[0].message
    );
}

#[test]
fn enforcement_decides_which_rejections_abort_the_build() {
    let defs = [spin("spin")];
    let input = TerminationInput::from_meta(&HashMap::new());
    let report = input.check(&defs);

    let proofs_only = input.partition_errors(&report, &defs, Enforcement::ProofsOnly);
    assert!(proofs_only.gating.is_empty());
    assert_eq!(proofs_only.reported.len(), 1);

    let all = input.partition_errors(&report, &defs, Enforcement::All);
    assert_eq!(all.gating.len(), 1);
    assert!(all.reported.is_empty());

    let report_only = input.partition_errors(&report, &defs, Enforcement::Report);
    assert!(report_only.gating.is_empty());
    assert_eq!(report_only.reported.len(), 1);
}

#[test]
fn a_rejected_proof_gates_even_at_the_default_level() {
    let defs = [spin("recursive_proof")];
    let entries = meta(&[("recursive_proof", proof_meta())]);
    let input = TerminationInput::from_meta(&entries);
    let report = input.check(&defs);

    let split = input.partition_errors(&report, &defs, Enforcement::ProofsOnly);
    assert_eq!(split.gating.len(), 1);
    assert!(split.reported.is_empty());
}

#[test]
fn the_enforcement_override_wins_over_the_environment_and_the_default() {
    // `set_enforcement` is process-wide, so this test owns the global for its
    // duration and drops the override before returning. It must *reset* rather
    // than store the default back (ADR 11.8.26b): storing a level pins it, and
    // once `All` became the default, "restoring" `ProofsOnly` would have left
    // every later test in this binary running against a weakened gate.
    //
    // "Owns" is enforced, not hoped for (ADR 12.8.26a §5.1): the other holder is
    // `doctor::checks::check_tool_reachability`, whose probes elaborate under a
    // `ReportingOnly` guard in the same test binary.
    let _enforcement = super::lock_enforcement();
    assert_eq!(super::enforcement(), Enforcement::All, "the default");

    for level in [
        Enforcement::All,
        Enforcement::Report,
        Enforcement::ProofsOnly,
    ] {
        super::set_enforcement(level);
        assert_eq!(super::enforcement(), level);
    }

    // The loop deliberately ends on a NON-default level. Ending on `All` would
    // leave the assertion below true whether `reset_enforcement` did anything
    // or nothing at all — which is how it read until the close-out mutation
    // sweep flagged `replace reset_enforcement with ()` as surviving.
    super::reset_enforcement();
    assert_eq!(
        super::enforcement(),
        Enforcement::All,
        "back to the default"
    );
}

#[test]
fn proof_relevance_is_recorded_for_every_proof_item_and_annotated_function() {
    use crate::ast::{Item, SourceFile};
    use tungsten_core::Context;

    let (ast, parse_errors): (SourceFile, _) = crate::parse(
        "#[partial]\n\
         fn opted_out(n: Nat) -> Nat { n }\n\
         fn plain(n: Nat) -> Nat { n }\n\
         theorem thm(): Nat { 0 }\n",
    );
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    assert!(ast
        .items
        .iter()
        .any(|item| matches!(item, Item::Theorem(_))));

    let ctx = Box::leak(Box::new(Context::new()));
    let mut elaborator = crate::elaborate::Elaborator::new(ctx);
    elaborator
        .elaborate_file(&ast)
        .unwrap_or_else(|errors| panic!("elaboration failed: {errors:?}"));
    let recorded = &elaborator.termination_meta;

    assert!(recorded["opted_out"].attrs.partial);
    assert!(!recorded["opted_out"].is_proof);
    assert!(recorded["thm"].is_proof);
    assert!(!recorded["thm"].attrs.partial);
    // An unannotated executable function costs no entry.
    assert!(!recorded.contains_key("plain"));
}
