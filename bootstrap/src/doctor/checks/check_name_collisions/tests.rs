//! Tests: the census — which names collide, how they are classified, and which
//! definition the flat table currently picks.
//!
//! Every test drives the pure `census` over a hand-built `ModuleInfo`, so
//! nothing here parses a file or elaborates one. Rendering is next door in
//! `report_tests.rs`; the end-to-end binding — that the check really does run
//! on a file the elaborator rejects — is the `name-collisions` row in
//! `check_tool_reachability`'s `PAIRINGS`.

use crate::ast::Visibility;

use super::census::{CollisionClass, ReexportHandling, Severity, Winner};
use super::report::render_human;
use super::select;
use super::test_support::{path, TreeBuilder};

/// The headline case, and the shape ADR 7.8.26b hit three times: a new private
/// definition registered last, shadowing a pre-existing one.
#[test]
fn a_private_definition_shadowing_another_is_the_live_class() {
    let tree = TreeBuilder::default()
        .define("elab::error::source", "string_slice", Visibility::Private)
        .define(
            "driver::pipeline::test::harness::rewrite",
            "string_slice",
            Visibility::Private,
        );

    let census = tree.census(ReexportHandling::Subtract);

    assert_eq!(census.collisions.len(), 1, "one name, two modules");
    let finding = &census.collisions[0];
    assert_eq!(finding.name, "string_slice");
    assert_eq!(finding.class, CollisionClass::PrivateShadowed);
    assert_eq!(
        finding.sites.len(),
        2,
        "both modules named — the first fact E0016 withholds"
    );
    assert_eq!(
        finding.winner,
        Winner::Definition(path("driver::pipeline::test::harness::rewrite")),
        "the walk's last registration wins — the second fact E0016 withholds"
    );
}

/// Class (a): both `pub`, so nothing is an error today and the finding is
/// latent. `--severity live` must drop it and `all` must keep it.
#[test]
fn two_public_definitions_are_latent_and_only_severity_all_reports_them() {
    let tree = TreeBuilder::default()
        .define("driver::util", "render", Visibility::Public)
        .define("elab::report", "render", Visibility::Public);

    let census = tree.census(ReexportHandling::Subtract);

    assert_eq!(census.collisions[0].class, CollisionClass::LatentPublic);
    assert_eq!(select(&census, Severity::All).len(), 1);
    assert_eq!(
        select(&census, Severity::Live).len(),
        0,
        "latent is not an error today"
    );
}

/// Class (c): the Tungsten identifier IS the C symbol, so an `extern` pair is a
/// duplicate link symbol no visibility change can fix. It outranks the private
/// class, because that is the fix that actually applies.
#[test]
fn an_extern_pair_outranks_the_private_class() {
    let tree = TreeBuilder::default()
        .define_extern("driver::ffi::diagnostics", "tg_report", Visibility::Private)
        .define_extern(
            "driver::ffi::diagnostics_dev",
            "tg_report",
            Visibility::Public,
        );

    let census = tree.census(ReexportHandling::Subtract);

    assert_eq!(census.collisions[0].class, CollisionClass::ExternSymbol);
    assert!(
        CollisionClass::ExternSymbol.is_live() && CollisionClass::PrivateShadowed.is_live(),
        "both are errors today"
    );
    assert!(!CollisionClass::LatentPublic.is_live());
}

/// A single `extern` definition colliding with a plain `fn` is NOT the link
/// class: there is only one C symbol. It falls through to whichever of the
/// other two classes visibility says.
#[test]
fn one_extern_against_a_plain_fn_is_not_the_link_class() {
    let tree = TreeBuilder::default()
        .define_extern("driver::ffi", "tg_report", Visibility::Public)
        .define("elab::report", "tg_report", Visibility::Private);

    let census = tree.census(ReexportHandling::Subtract);

    assert_eq!(census.collisions[0].class, CollisionClass::PrivateShadowed);
}

/// D1b: a re-exported name is ONE definition reachable by several paths, which
/// is `reexport-completeness`'s question. Subtracting the synthesized copies is
/// what keeps `driver::util`'s chain from reporting.
#[test]
fn a_reexport_chain_is_not_a_collision() {
    let tree = TreeBuilder::default()
        .define(
            "driver::util::strings::convert",
            "nat_to_string",
            Visibility::Public,
        )
        .reexport(
            "driver::util::strings",
            "nat_to_string",
            "driver::util::strings::convert",
        )
        .reexport("driver::util", "nat_to_string", "driver::util::strings");

    let subtracted = tree.census(ReexportHandling::Subtract);
    assert_eq!(
        subtracted.collisions.len(),
        0,
        "one definition, three reachable paths"
    );
    assert_eq!(subtracted.definitions_considered, 1);

    // The measurement arm (AC 1): keeping the copies is what makes the chain
    // look like a three-way collision, so the difference between the two runs
    // IS the re-export class.
    let kept = tree.census(ReexportHandling::Keep);
    assert_eq!(kept.collisions.len(), 1);
    assert_eq!(kept.collisions[0].sites.len(), 3);
    assert_eq!(kept.definitions_considered, 3);
}

/// §5's precise case: a module that genuinely defines `foo` AND re-exports a
/// `foo` records no provenance entry (the copy is guarded), so the real
/// definition cannot be subtracted away.
#[test]
fn a_definition_is_never_subtracted_by_a_same_named_reexport() {
    // The guard in `copy_contents_entries` means the target keeps its own
    // definition and records nothing — modelled here by defining only.
    let tree = TreeBuilder::default()
        .define("elab::report", "render", Visibility::Private)
        .define("driver::util", "render", Visibility::Private);

    let census = tree.census(ReexportHandling::Subtract);
    assert_eq!(census.collisions.len(), 1);
}

/// The flat table is shared with types, so its entry for a name can point at a
/// module that defines no *value* of that name. Reported rather than silently
/// printing a winner that is not one of the candidates.
#[test]
fn a_winner_outside_the_candidate_set_is_named_as_foreign() {
    let tree = TreeBuilder::default()
        .define("elab::report", "Pattern", Visibility::Private)
        .define("driver::util", "Pattern", Visibility::Private)
        // A same-named *type* registered later takes the flat table's slot.
        .shadow_flat_entry("Pattern", "ast::pattern");

    let census = tree.census(ReexportHandling::Subtract);

    assert_eq!(
        census.collisions[0].winner,
        Winner::Foreign(path("ast::pattern"))
    );
    let rendered = render_human(&census, &select(&census, Severity::All), Severity::All);
    assert!(
        rendered.contains("defines no value of that name"),
        "the surprise is named, not hidden: {rendered}"
    );
}

/// `pub(crate)` is NOT the private class. A Tungsten program is one crate, so a
/// losing `pub(crate)` definition is still reachable and raises no E0016 — the
/// collision is latent, like a `pub` pair's. Pinned because `classify` decides
/// it in a bare `else`, where a reader cannot tell a decision from an oversight.
#[test]
fn a_crate_visible_definition_is_latent_not_the_private_class() {
    let both_crate = TreeBuilder::default()
        .define("elab::report", "render", Visibility::Crate)
        .define("driver::util", "render", Visibility::Crate)
        .census(ReexportHandling::Subtract);
    assert_eq!(
        both_crate.collisions[0].class,
        CollisionClass::LatentPublic,
        "pub(crate) is reachable crate-wide, so nothing is shadowed today"
    );
    assert_eq!(select(&both_crate, Severity::Live).len(), 0);

    // Mixed with `pub` is still latent — the rule is about `Private` alone.
    let mixed = TreeBuilder::default()
        .define("elab::report", "render", Visibility::Crate)
        .define("driver::util", "render", Visibility::Public)
        .census(ReexportHandling::Subtract);
    assert_eq!(mixed.collisions[0].class, CollisionClass::LatentPublic);

    // One private definition alongside it DOES trigger the live class, which is
    // what makes the two assertions above statements about `Crate` rather than
    // about the classifier being stuck.
    let with_private = TreeBuilder::default()
        .define("elab::report", "render", Visibility::Crate)
        .define("driver::util", "render", Visibility::Private)
        .census(ReexportHandling::Subtract);
    assert_eq!(
        with_private.collisions[0].class,
        CollisionClass::PrivateShadowed
    );
}

/// The `Winner` the flat table records is normally one of the definitions, but
/// the type exists for the case where no entry was recorded at all. Exercised
/// so the arm is not carried untested on the strength of "it cannot happen".
#[test]
fn a_name_absent_from_the_flat_table_has_no_recorded_winner() {
    let mut tree = TreeBuilder::default()
        .define("elab::report", "render", Visibility::Private)
        .define("driver::util", "render", Visibility::Private);
    tree.forget_flat_entry("render");

    let census = tree.census(ReexportHandling::Subtract);
    assert_eq!(census.collisions[0].winner, Winner::Unrecorded);
    assert_eq!(
        census.collisions[0].sites.len(),
        2,
        "both definitions are still reported — only the winner is unknown"
    );
}
