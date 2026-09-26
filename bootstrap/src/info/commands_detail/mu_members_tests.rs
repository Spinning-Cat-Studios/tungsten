//! Tests for `tungsten info type mu-members`.
//!
//! The resolution is asserted over in-memory `Type` values — no filesystem, no
//! elaborated project. What matters is that a binder reported `Resolvable` is
//! one synthesis can actually use, and that the three *unusable* shapes stay
//! distinguishable: they have different fixes.
//!
//! Tests: bootstrap/src/info/commands_detail/mu_members.rs

use super::*;

use std::collections::HashMap;

use tungsten_bootstrap::driver::RecordTypes;
use tungsten_bootstrap::elaborate::{AdtOrigin, TypeProvenance};
use tungsten_bootstrap::scratch::ScratchDir;

fn binder(name: &str, body: Type) -> Type {
    Type::Mu(name.to_string(), Box::new(body))
}

/// A cluster member in the encoder's nested-binder form.
fn two_binder_chain() -> Type {
    binder("α_Self", binder("α_Other", Type::Unit))
}

fn provenance(entries: &[(&str, &str)]) -> TypeProvenance {
    let mut p = TypeProvenance::default();
    for (b, adt) in entries {
        p.mu_origins.insert(
            (*b).to_string(),
            AdtOrigin {
                adt_name: (*adt).to_string(),
                type_args: vec![],
                constructors: vec![],
            },
        );
    }
    p
}

/// `resolve_chain` needs a `ProjectOutput`, which is large; only two of its
/// fields are read, so the rest are defaulted.
fn project_with(
    encoded: HashMap<String, Type>,
    provenance: TypeProvenance,
) -> (ProjectOutput, ComparatorTypes) {
    let types = ComparatorTypes::new(
        RecordTypes::new(),
        &encoded,
        &provenance,
        HashMap::new(),
        &HashMap::new(),
    );
    let project = ProjectOutput {
        defs: Vec::new(),
        codegen_units: Vec::new(),
        record_types: RecordTypes::new(),
        adt_types: HashMap::new(),
        type_aliases: HashMap::new(),
        type_provenance: provenance,
        source_map: Default::default(),
        encoded_types: encoded,
        mutual_recursion_groups: HashMap::new(),
        type_visibilities: HashMap::new(),
        record_field_visibilities: HashMap::new(),
        value_import_targets: Default::default(),
        termination_meta: Default::default(),
    };
    (project, types)
}

#[test]
fn a_non_recursive_type_has_no_chain() {
    let (project, types) = project_with(HashMap::new(), TypeProvenance::default());
    assert!(resolve_chain(&Type::Nat, &project, &types).is_empty());
}

#[test]
fn the_outermost_binder_is_always_the_type_itself() {
    // It needs no lookup — `mu_comparator` substitutes the operand directly —
    // so reporting it as unresolvable would be a false alarm on every type.
    let (project, types) = project_with(HashMap::new(), provenance(&[("α_Self", "Self")]));
    let chain = resolve_chain(&binder("α_Self", Type::Unit), &project, &types);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].verdict, BinderVerdict::SelfReference);
}

#[test]
fn an_inner_binder_with_a_stored_member_encoding_is_resolvable() {
    let mut encoded = HashMap::new();
    encoded.insert("Other".to_string(), Type::Nat);
    let (project, types) = project_with(
        encoded,
        provenance(&[("α_Self", "Self"), ("α_Other", "Other")]),
    );
    let chain = resolve_chain(&two_binder_chain(), &project, &types);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].verdict, BinderVerdict::SelfReference);
    let BinderVerdict::Resolvable { encoding } = &chain[1].verdict else {
        panic!("a member with a stored encoding resolves: {:?}", chain[1]);
    };
    assert!(encoding.contains("Nat"), "{encoding}");
}

/// The generic-ADT case: provenance names it, but there is no monomorphic
/// stored encoding to resolve to. Distinct from "nothing knows about it",
/// because the fixes differ — this one is expected and harmless.
#[test]
fn an_inner_binder_for_a_generic_adt_is_reported_as_generic() {
    let (project, types) = project_with(
        HashMap::new(),
        provenance(&[("α_Self", "Self"), ("α_List", "List")]),
    );
    let chain = resolve_chain(
        &binder("α_Self", binder("α_List", Type::Unit)),
        &project,
        &types,
    );
    assert_eq!(
        chain[1].verdict,
        BinderVerdict::Generic {
            adt: "List".to_string()
        }
    );
}

#[test]
fn an_inner_binder_with_no_provenance_at_all_is_unresolvable() {
    let (project, types) = project_with(HashMap::new(), provenance(&[("α_Self", "Self")]));
    let chain = resolve_chain(&two_binder_chain(), &project, &types);
    assert_eq!(chain[1].verdict, BinderVerdict::Unresolvable);
    assert_eq!(chain[1].origin, None);
}

// ── the report ──────────────────────────────────────────────────────────────

#[test]
fn a_single_binder_report_says_there_is_nothing_to_resolve() {
    let chain = vec![BinderResolution {
        binder: "α_List".to_string(),
        origin: Some("List".to_string()),
        verdict: BinderVerdict::SelfReference,
    }];
    let out = render_report("List", &chain);
    assert!(out.contains("ordinary self-recursion"), "{out}");
    assert!(
        !out.contains("does not"),
        "the sibling-bodies warning is only about clusters: {out}"
    );
}

/// The load-bearing sentence: without it a reader takes the encoding at face
/// value and reaches a confident wrong conclusion, which is exactly what
/// happened during ADR 1.8.26b's D2 diagnosis.
#[test]
fn a_cluster_report_states_that_sibling_bodies_are_not_carried() {
    let chain = vec![
        BinderResolution {
            binder: "α_Alpha".to_string(),
            origin: Some("Alpha".to_string()),
            verdict: BinderVerdict::SelfReference,
        },
        BinderResolution {
            binder: "α_Beta".to_string(),
            origin: Some("Beta".to_string()),
            verdict: BinderVerdict::Resolvable {
                encoding: "μα_Beta. …".to_string(),
            },
        },
    ];
    let out = render_report("Alpha", &chain);
    assert!(out.contains("does not"), "{out}");
    assert!(out.contains("carry the other members' bodies"), "{out}");
    assert!(out.contains("α_Beta"), "{out}");
}

#[test]
fn unresolvable_binders_are_called_out_with_the_follow_up_command() {
    let chain = vec![
        BinderResolution {
            binder: "α_Alpha".to_string(),
            origin: Some("Alpha".to_string()),
            verdict: BinderVerdict::SelfReference,
        },
        BinderResolution {
            binder: "α_Ghost".to_string(),
            origin: None,
            verdict: BinderVerdict::Unresolvable,
        },
    ];
    let out = render_report("Alpha", &chain);
    assert!(out.contains("cannot be synthesized past them"), "{out}");
    assert!(out.contains("α_Ghost"), "{out}");
    assert!(out.contains("doctor check comparable Alpha"), "{out}");
}

/// …and a fully resolvable cluster must NOT print the failure section, or the
/// warning becomes noise a reader learns to skip.
#[test]
fn a_fully_resolvable_cluster_reports_no_unresolved_binders() {
    let chain = vec![
        BinderResolution {
            binder: "α_Alpha".to_string(),
            origin: Some("Alpha".to_string()),
            verdict: BinderVerdict::SelfReference,
        },
        BinderResolution {
            binder: "α_Beta".to_string(),
            origin: Some("Beta".to_string()),
            verdict: BinderVerdict::Resolvable {
                encoding: "μα_Beta. …".to_string(),
            },
        },
    ];
    let out = render_report("Alpha", &chain);
    assert!(!out.contains("cannot be synthesized past them"), "{out}");
}

// ── the right-hand column, and the CLI exit code ────────────────────────────
//
// `describe` renders the column a reader actually scans, and the four verdicts
// must not collapse into one another — a renderer returning a constant would
// leave every binder looking alike while the report still "worked".

#[test]
fn each_verdict_describes_itself_distinctly() {
    let entry = |verdict, origin: Option<&str>| BinderResolution {
        binder: "α_X".to_string(),
        origin: origin.map(str::to_string),
        verdict,
    };
    let rendered = [
        describe(&entry(BinderVerdict::SelfReference, Some("X"))),
        describe(&entry(
            BinderVerdict::Resolvable {
                encoding: "μα_Y. Nat".to_string(),
            },
            Some("Y"),
        )),
        describe(&entry(
            BinderVerdict::Generic {
                adt: "List".to_string(),
            },
            Some("List"),
        )),
        describe(&entry(BinderVerdict::Unresolvable, None)),
    ];
    for (i, a) in rendered.iter().enumerate() {
        assert!(!a.is_empty(), "an empty column tells a reader nothing");
        for b in rendered.iter().skip(i + 1) {
            assert_ne!(a, b, "two verdicts render identically");
        }
    }
    assert!(rendered[0].contains("this type"), "{}", rendered[0]);
    assert!(rendered[1].contains("μα_Y. Nat"), "{}", rendered[1]);
    assert!(rendered[2].contains("generic"), "{}", rendered[2]);
    assert!(rendered[3].contains("UNRESOLVED"), "{}", rendered[3]);
}

/// A binder with no provenance must not render a blank origin — `<no
/// provenance>` is a fact the reader can act on; an empty string is not.
#[test]
fn a_binder_without_provenance_says_so_rather_than_rendering_blank() {
    let out = describe(&BinderResolution {
        binder: "α_Ghost".to_string(),
        origin: None,
        verdict: BinderVerdict::SelfReference,
    });
    assert!(out.contains("<no provenance>"), "{out}");
}

#[test]
fn the_cli_entry_fails_on_a_file_it_cannot_elaborate() {
    // `ExitCode::default()` is SUCCESS, so a body replaced wholesale is
    // invisible unless a failing path is asserted.
    let scratch = ScratchDir::new("mu-members-badfile");
    let path = scratch.join("broken.tg");
    std::fs::write(&path, "this is not valid tungsten source {{{").unwrap();
    assert_eq!(
        cmd_info_mu_members("Whatever", &path, false, 1),
        ExitCode::FAILURE
    );
}

#[test]
fn the_cli_entry_fails_on_a_type_the_file_does_not_declare() {
    let scratch = ScratchDir::new("mu-members-unknown");
    let path = scratch.join("ok.tg");
    std::fs::write(&path, "type Known = | A | B\nfn main() -> Nat { 0 }\n").unwrap();
    assert_eq!(
        cmd_info_mu_members("NoSuchType", &path, false, 1),
        ExitCode::FAILURE
    );
}
