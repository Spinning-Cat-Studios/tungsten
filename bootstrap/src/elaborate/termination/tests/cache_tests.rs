//! Tests: bootstrap/src/elaborate/termination/cache.rs
//!
//! The warm-cache half of the gate adapter, split from the sibling module when
//! the combined file approached the size limit. The seam mirrors the source's:
//! everything here is about `CachedTermination` — what a module contributes to
//! it, what survives the warm/cold boundary, and when a carried fact still
//! counts as something to report. The adapter's own conversion, rendering and
//! enforcement tests stay next door.
//!
//! Fixtures (`spin`, `meta`, `partial_meta`, `proof_meta`) come from the parent
//! test module rather than being duplicated.

use std::collections::HashMap;

use tungsten_core::terms::Term;
use tungsten_core::types::Type;

use super::{def, meta, partial_meta, proof_meta, spin};
use crate::elaborate::termination::{has_nothing_to_report, CachedTermination, TerminationInput};

#[test]
fn cached_facts_carry_a_modules_rejections_and_its_taint() {
    let defs = [spin("spin")];
    let entries = meta(&[("spin", partial_meta())]);
    let cached = CachedTermination::from_module(&defs, &entries);

    assert!(cached.failures.is_empty(), "a partial def is not rejected");
    assert_eq!(cached.partial, vec!["spin".to_string()]);
    assert_eq!(cached.mentions.len(), 1);
    assert!(!cached.is_empty());

    let carried = cached.as_carried();
    assert!(carried.partial.contains("spin"));
    assert!(carried.mentions.contains_key("spin"));
}

#[test]
fn a_cached_rejection_is_remembered_with_its_proof_relevance() {
    let defs = [spin("recursive_proof")];
    let entries = meta(&[("recursive_proof", proof_meta())]);
    let cached = CachedTermination::from_module(&defs, &entries);

    assert_eq!(cached.failures.len(), 1);
    assert!(cached.failures[0].1, "a rejected proof is proof-relevant");
    assert_eq!(cached.proofs, vec!["recursive_proof".to_string()]);
}

#[test]
fn taint_crosses_the_warm_cold_boundary_through_the_carried_facts() {
    // `spin` is `#[partial]` and lives in a cached module; the proof is fresh.
    let cached =
        CachedTermination::from_module(&[spin("spin")], &meta(&[("spin", partial_meta())]));
    let fresh = [def(
        "thm",
        Type::Prop,
        Term::App(
            Box::new(Term::Global("spin".to_string())),
            Box::new(Term::Unit),
        ),
    )];
    let input = TerminationInput::from_meta(&meta(&[("thm", proof_meta())]));

    let blind = input.check(&fresh);
    assert!(
        blind.is_clean(),
        "without the carried facts the taint is invisible — this is the hole"
    );

    let carried = input.check_with_carried(&fresh, &cached.as_carried());
    assert_eq!(carried.failures.len(), 1);
    assert_eq!(carried.failures[0].function, "thm");
}

#[test]
fn absorbing_module_facts_accumulates_every_field() {
    let mut project = CachedTermination::default();
    assert!(project.is_empty());

    project.absorb(CachedTermination::from_module(
        &[spin("spin")],
        &meta(&[("spin", partial_meta())]),
    ));
    project.absorb(CachedTermination::from_module(
        &[spin("recursive_proof")],
        &meta(&[("recursive_proof", proof_meta())]),
    ));

    assert_eq!(project.partial, vec!["spin".to_string()]);
    assert_eq!(project.proofs, vec!["recursive_proof".to_string()]);
    assert_eq!(project.failures.len(), 1);
    assert_eq!(project.mentions.len(), 2);
}

#[test]
fn a_clean_report_with_no_carried_rejections_has_nothing_to_report() {
    let report = TerminationInput::from_meta(&HashMap::new()).check(&[]);

    assert!(has_nothing_to_report(
        &report,
        &CachedTermination::default()
    ));
}

#[test]
fn a_carried_rejection_is_still_something_to_report_on_a_clean_run() {
    // The fresh side is clean because the rejected module came from cache and
    // contributed no terms — reading only `report.is_clean()` drops it.
    let carried = CachedTermination::from_module(
        &[spin("recursive_proof")],
        &meta(&[("recursive_proof", proof_meta())]),
    );
    let report = TerminationInput::from_meta(&HashMap::new()).check(&[]);

    assert!(report.is_clean());
    assert!(!has_nothing_to_report(&report, &carried));
}

#[test]
fn a_fresh_rejection_is_something_to_report_even_with_nothing_carried() {
    let report = TerminationInput::from_meta(&HashMap::new()).check(&[spin("spin")]);

    assert!(!has_nothing_to_report(
        &report,
        &CachedTermination::default()
    ));
}

#[test]
fn cached_facts_are_empty_only_when_every_field_is() {
    assert!(CachedTermination::default().is_empty());

    let mut only_partial = CachedTermination::default();
    only_partial.partial.push("spin".to_string());
    assert!(!only_partial.is_empty());

    let mut only_proofs = CachedTermination::default();
    only_proofs.proofs.push("thm".to_string());
    assert!(!only_proofs.is_empty());

    let mut only_mentions = CachedTermination::default();
    only_mentions.mentions.push(("f".to_string(), Vec::new()));
    assert!(!only_mentions.is_empty());
}
