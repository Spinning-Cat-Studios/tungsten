//! Reconciliation of the `info pipeline` inventory against the clap command
//! tree (ADR 28.7.26f D2), in both directions:
//!
//! - **Completeness** — every non-hidden leaf subcommand is *classified*: it has
//!   either a documented entry or an explicit `NotDiagnostic { why }`. Without
//!   this, a new tool ships invisible and an agent reads source instead.
//! - **No phantoms** — every `Subcommand` entry names a path that resolves.
//!   Without this, an entry outlives a rename and the agent runs a command that
//!   errors, then distrusts the whole inventory.
//!
//! Both arms must hold in **both feature configurations** (D7). Subcommand
//! *variants* are `#[cfg(feature = "codegen")]`-gated, so under
//! `--no-default-features` the codegen entries would all read as phantoms, while
//! a codegen-only run would let LLVM-free drift through. The asymmetry also cuts
//! the other way: the completeness arm is *weaker* without codegen, since a
//! codegen-gated subcommand is absent from the tree and cannot be reported
//! unclassified — which is precisely why `doctor check link-collisions` went
//! undetected. `cargo test -p tungsten_bootstrap --no-default-features` (CI) and
//! `make test-codegen` are the two named gates.
//!
//! The reconciliations against sources of truth OUTSIDE the clap tree — a
//! command's accepted flags, the `.mk` files' declared targets — and the
//! rendered/JSON surfaces are in [`surfaces`].

mod surfaces;

use std::collections::HashSet;

use clap::CommandFactory;

use super::entry::{CostTier, EntryKind, PipelineEntry};
use super::inventory::{all_entries, SECTIONS};
use super::reconcile::flags::{accepted_long_flags, documented_flag_name};
use super::reconcile::make_targets;
use crate::cli::Cli;
use crate::list_commands::leaf_paths;

/// Whether this build's clap tree contains the codegen-gated subcommands.
const TREE_HAS_CODEGEN: bool = cfg!(feature = "codegen");

fn tree_leaves() -> HashSet<String> {
    leaf_paths(&Cli::command()).into_iter().collect()
}

/// Every path the inventory classifies, whatever the classification.
fn classified_paths() -> HashSet<&'static str> {
    all_entries()
        .filter(|e| {
            matches!(
                e.kind,
                EntryKind::Subcommand | EntryKind::NotDiagnostic { .. }
            )
        })
        .map(|e| e.path)
        .collect()
}

fn documented_subcommands() -> impl Iterator<Item = &'static PipelineEntry> {
    all_entries().filter(|e| e.kind == EntryKind::Subcommand)
}

// ── D2.1 Completeness: no subcommand ships invisible ──

#[test]
fn every_clap_leaf_is_classified_by_the_inventory() {
    let classified = classified_paths();
    let mut unclassified: Vec<String> = tree_leaves()
        .into_iter()
        .filter(|leaf| !classified.contains(leaf.as_str()))
        .collect();
    unclassified.sort();
    assert!(
        unclassified.is_empty(),
        "these subcommands exist but `tungsten info pipeline` neither documents \
         nor classifies them — add a PipelineEntry, or a NotDiagnostic entry \
         stating why it is out of scope (ADR 28.7.26f D2a): {unclassified:#?}"
    );
}

/// ADR 5.9.26f AC5 — `info module dependents` is listed, at cost 3.
///
/// The two arms either side of this one are generic over the whole inventory,
/// and both stay green when an entry is missing from the inventory *and* from
/// the clap tree — the completeness arm has nothing to report, the phantom arm
/// nothing to resolve. Neither reads a cost tier at all. So the one thing ADR
/// 5.9.26f's criterion actually claims — this command, at this tier, reachable
/// — is asserted here by name.
#[test]
fn info_module_dependents_is_listed_at_the_elaborate_tier() {
    let listed = documented_subcommands()
        .find(|candidate| candidate.path == "info module dependents")
        .expect(
            "`tungsten info pipeline` does not list `info module dependents` — \
             add its PipelineEntry to inventory/info_commands.rs (ADR 5.9.26f)",
        );
    assert_eq!(
        listed.cost,
        Some(CostTier::Elaborate),
        "`info module dependents` elaborates the corpus before it can invert \
         the import table (ADR 5.9.26f D4), so it is cost 3 and the inventory \
         must say so"
    );
    assert!(
        !listed.requires_codegen && tree_leaves().contains("info module dependents"),
        "the entry must resolve in the LLVM-free build too — the useful moment \
         is before a regroup, on a tree that compiles, not behind a codegen gate"
    );
}

// ── D2.2 No phantoms: no entry outlives a rename ──

#[test]
fn every_documented_subcommand_resolves_in_the_clap_tree() {
    let leaves = tree_leaves();
    let mut phantoms: Vec<&str> = documented_subcommands()
        .filter(|e| TREE_HAS_CODEGEN || !e.requires_codegen)
        .map(|e| e.path)
        .filter(|path| !leaves.contains(*path))
        .collect();
    phantoms.sort_unstable();
    assert!(
        phantoms.is_empty(),
        "`tungsten info pipeline` advertises subcommands that do not resolve — \
         an agent following the inventory would run a command that errors \
         (codegen in this build: {TREE_HAS_CODEGEN}): {phantoms:#?}"
    );
}

#[test]
fn codegen_gated_entries_are_labelled_on_the_side_of_the_gate_they_are_on() {
    // The inverse of the phantom check, and the reason a mislabelled entry
    // cannot hide: in the LLVM-free build a `requires_codegen` entry must NOT
    // resolve, and in the codegen build every entry must.
    let leaves = tree_leaves();
    let mut mislabelled: Vec<&str> = documented_subcommands()
        .filter(|e| e.requires_codegen && leaves.contains(e.path) != TREE_HAS_CODEGEN)
        .map(|e| e.path)
        .collect();
    mislabelled.sort_unstable();
    assert!(
        mislabelled.is_empty(),
        "entries marked `requires_codegen` disagree with this build's clap tree \
         (codegen: {TREE_HAS_CODEGEN}): {mislabelled:#?}"
    );
}

/// **ADR 7.9.26c AC 4** — the `info type spine` arm is discoverable: it carries
/// a documented `PipelineEntry` at the elaborate tier and that path resolves in
/// the clap tree.
///
/// The two reconciliation walks above already assert both properties over the
/// *whole* inventory, so this adds no new rule — what it adds is a **name**.
/// Deleting the entry and the subcommand together keeps every count consistent
/// and every walk green; it fails here, where the test says which tool went
/// missing.
#[test]
fn the_record_spine_arm_is_documented_and_resolves() {
    let entry = documented_subcommands()
        .find(|e| e.path == "info type spine")
        .expect("`info type spine` carries a PipelineEntry (ADR 7.9.26c AC 4)");
    assert_eq!(
        entry.cost,
        Some(CostTier::Elaborate),
        "the spine walk reads an elaborated project — cost 3, not 1 or 4"
    );
    assert!(
        tree_leaves().contains("info type spine"),
        "the documented path must resolve in the clap tree"
    );
}

/// Cross-references legitimately point at codegen-only tools, so only the
/// codegen tree can tell a dangling link from a cfg'd-out one. Gated at compile
/// time rather than behind a runtime `if`: a test that computes a verdict and
/// then declines to assert it in half its runs looks like coverage and is not.
#[cfg(feature = "codegen")]
#[test]
fn see_also_cross_references_resolve_in_the_clap_tree() {
    let leaves = tree_leaves();
    let mut dangling: Vec<&str> = all_entries()
        .flat_map(|e| e.see_also.iter().copied())
        .filter(|target| !leaves.contains(*target))
        .collect();
    dangling.sort_unstable();
    dangling.dedup();
    assert!(
        all_entries().any(|e| !e.see_also.is_empty()),
        "no cross-references in the inventory — a vacuous pass"
    );
    assert!(
        dangling.is_empty(),
        "`see also` names subcommands that do not exist: {dangling:#?}"
    );
}

// ── D5 Cost annotations are part of the contract ──

#[test]
fn every_documented_subcommand_carries_a_cost_tier() {
    let mut untiered: Vec<&str> = documented_subcommands()
        .filter(|e| e.cost.is_none())
        .map(|e| e.path)
        .collect();
    untiered.sort_unstable();
    assert!(
        untiered.is_empty(),
        "a missing cost tier misroutes an agent into an expensive path, which is \
         the same harm as a missing entry (ADR 28.7.26f D5): {untiered:#?}"
    );
}

#[test]
fn a_section_cost_hint_names_its_own_default_tier() {
    for section in SECTIONS {
        let Some(default) = section.default_cost else {
            continue;
        };
        assert!(
            section.cost_hint.contains(&default.tier().to_string()),
            "section {:?} advertises {:?} but defaults to cost {}",
            section.title,
            section.cost_hint,
            default.tier()
        );
    }
}

// ── Internal consistency of the table itself ──

#[test]
fn a_subcommand_usage_line_begins_with_its_reconciliation_path() {
    let mut mismatched: Vec<(&str, &str)> = documented_subcommands()
        .filter(|e| !e.usage.starts_with(&format!("tungsten {}", e.path)))
        .map(|e| (e.path, e.usage))
        .collect();
    mismatched.sort_unstable();
    assert!(
        mismatched.is_empty(),
        "`usage` is the display form of `path`; if they disagree the reader and \
         the reconciliation key have parted company: {mismatched:#?}"
    );
}

#[test]
fn no_subcommand_path_carries_a_placeholder_or_flag() {
    let mut malformed: Vec<&str> = documented_subcommands()
        .map(|e| e.path)
        .filter(|path| path.contains('<') || path.starts_with('-'))
        .collect();
    malformed.sort_unstable();
    assert!(
        malformed.is_empty(),
        "a reconciliation key must be placeholder- and flag-free: {malformed:#?}"
    );
}

#[test]
fn no_clap_leaf_is_both_documented_and_declared_non_diagnostic() {
    let documented: HashSet<&str> = documented_subcommands().map(|e| e.path).collect();
    let mut contradictory: Vec<&str> = all_entries()
        .filter(|e| matches!(e.kind, EntryKind::NotDiagnostic { .. }))
        .map(|e| e.path)
        .filter(|path| documented.contains(path))
        .collect();
    contradictory.sort_unstable();
    assert!(
        contradictory.is_empty(),
        "classified as NOT a diagnostic while also being documented as one: {contradictory:#?}"
    );
}

#[test]
fn every_not_diagnostic_classification_states_a_reason() {
    for entry in all_entries() {
        if let EntryKind::NotDiagnostic { why } = entry.kind {
            assert!(
                !why.trim().is_empty(),
                "`{}` is excluded with no stated reason — that is an exclusion \
                 list by another name (ADR 28.7.26f D2a)",
                entry.path
            );
        }
    }
}
