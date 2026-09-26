//! Layer 2 of the prose check: the real surfaces, against the real withheld set.
//!
//! Thin by design. Every judgement lives in [`super::predicate`]; this module
//! only decides *which* strings describe the listing, renders them, and reads
//! the production `UNLISTED_KINDS` they are checked against.

use clap::CommandFactory;

use super::predicate::{
    claim_is_consistent, exhaustiveness_claims, normalize_whitespace, ClaimStrictness,
};
use crate::cli::Cli;
use crate::explain::error_catalogue;
use crate::explain::tests::catalogue::DELIBERATELY_UNLISTED;

/// `explain error`'s **rendered** long help — what a user reads, not the doc
/// comment that produced it. A doc comment that renders differently than it
/// reads cannot slip past this.
fn rendered_explain_error_help() -> String {
    let root = Cli::command();
    let explain = root
        .find_subcommand("explain")
        .expect("the `explain` namespace must exist");
    let mut error = explain
        .find_subcommand("error")
        .expect("`explain error` must exist")
        .clone();
    error.render_long_help().to_string()
}

/// The production withheld set, as `(kind name, code)`.
///
/// Keyed on `UNLISTED_KINDS` and not on [`DELIBERATELY_UNLISTED`, the copy that
/// sits beside these tests](DELIBERATELY_UNLISTED): a check whose source of
/// truth lives next to it stays consistent with that copy no matter what the
/// CLI does, which is precisely the defect ADR 19.8.26b exists to fix.
fn production_withheld_set() -> Vec<(&'static str, &'static str)> {
    error_catalogue::unlisted_kinds()
        .iter()
        .map(|kind| {
            let (_, _, code) = error_catalogue::entry_body_for_test(kind)
                .unwrap_or_else(|| panic!("`{kind}` is withheld but has no explanation"));
            (*kind, code)
        })
        .collect()
}

/// The repo-root AI documentation surfaces carrying this same claim.
///
/// Outside this crate, and prose no compiler reads — but the boundary this
/// check draws is the *claim*, not the file. The `info error-sites` hint
/// already proved that: it had gone stale in a second namespace, where no
/// review of `explain error` would have looked.
const DOCUMENTED_CLAIM_SURFACES: &[&str] =
    &[".claude/CLAUDE.md", ".github/copilot-instructions.md"];

/// The phrase anchoring the claim inside a documentation surface.
const DOC_CLAIM_ANCHOR: &str = "no argument lists";

/// The documentation surfaces present in this checkout.
///
/// Both are development-repository files the public repository does not carry
/// (ADR 25.9.26l), and a surface that is absent makes no claim — so there it is
/// not watched, rather than failing every public clone's `make test`. Where a
/// surface IS present, dropping its claim still panics in [`documented_claim`].
fn present_claim_surfaces() -> Vec<&'static str> {
    DOCUMENTED_CLAIM_SURFACES
        .iter()
        .copied()
        .filter(|relative_path| repo_root().join(relative_path).is_file())
        .collect()
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the bootstrap manifest dir sits under the repo root")
        .to_path_buf()
}

/// The one line `relative_path` makes the claim on.
///
/// Anchored rather than read whole: these files describe the entire toolchain,
/// so judging all of one would rule on forty unrelated claims. A surface that
/// no longer carries the anchor **panics** rather than quietly contributing
/// nothing — dropping the claim is fine, dropping it *here* while it lives on
/// in different words is the failure this check exists for.
fn documented_claim(relative_path: &str) -> String {
    let path = repo_root().join(relative_path);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the claim surface at {}: {e}", path.display()));
    text.lines()
        .find(|line| line.contains(DOC_CLAIM_ANCHOR))
        .unwrap_or_else(|| {
            panic!(
                "`{relative_path}` no longer contains `{DOC_CLAIM_ANCHOR}`. If the claim was \
                 reworded, re-anchor this check on the new wording; if it was removed, drop \
                 the surface from DOCUMENTED_CLAIM_SURFACES"
            )
        })
        .to_string()
}

/// Every prose description of the listing, with the strictness it is held to.
fn described_surfaces() -> Vec<(&'static str, ClaimStrictness, String)> {
    let inventory_entry = crate::info::commands::pipeline::entry_summary_for_test("explain error")
        .expect("`explain error` must have an `info pipeline` entry");
    let mut surfaces = vec![
        (
            "explain error --help",
            ClaimStrictness::Explains,
            rendered_explain_error_help(),
        ),
        (
            "info pipeline entry",
            ClaimStrictness::Explains,
            inventory_entry.to_string(),
        ),
        (
            "unknown-code advice",
            ClaimStrictness::Qualifies,
            error_catalogue::UNKNOWN_CODE_ADVICE.to_string(),
        ),
        (
            "unknown-name advice",
            ClaimStrictness::Qualifies,
            error_catalogue::UNKNOWN_NAME_ADVICE.to_string(),
        ),
        (
            "info error-sites hint",
            ClaimStrictness::Qualifies,
            crate::info::error_sites::EXPLAIN_LISTING_HINT.to_string(),
        ),
    ];
    // One line of prose each, so `Qualifies`: neither has room for the
    // paragraph `Explains` would demand, and growing one would be worse copy.
    surfaces.extend(
        present_claim_surfaces()
            .into_iter()
            .map(|path| (path, ClaimStrictness::Qualifies, documented_claim(path))),
    );
    surfaces
}

#[test]
fn no_description_of_the_listing_overpromises() {
    let withheld = production_withheld_set();
    for (surface, level, text) in described_surfaces() {
        // Both refusal reasons, spelled out. A bare "is inconsistent" sends the
        // reader hunting for a missing sentence that may be sitting right
        // there in words `EXCLUSION_MARKERS` does not know.
        assert!(
            claim_is_consistent(&withheld, level, &text),
            "`{surface}` is inconsistent with the withheld set {withheld:?} at \
             {level:?}. Either it claims the listing is exhaustive without the \
             `user-facing` qualifier, or — at Explains only — it never names an \
             entry as withheld. If it DOES name one, the wording may not match \
             `EXCLUSION_MARKERS` in predicate.rs; widen that list rather than \
             rewording good copy to suit it.\n{text}"
        );
    }
}

#[test]
fn every_described_surface_actually_makes_a_claim() {
    // Non-vacuity. Every surface in the set is there because it describes the
    // listing as exhaustive; one that stopped doing so would leave
    // `no_description_of_the_listing_overpromises` green over nothing.
    for (surface, _, text) in described_surfaces() {
        assert!(
            !exhaustiveness_claims(&normalize_whitespace(&text)).is_empty(),
            "`{surface}` no longer claims anything about the listing, so the \
             check above is vacuous for it:\n{text}"
        );
    }
}

/// Every surface this check is supposed to be watching, by name.
///
/// The check can only judge strings it is handed, so the roster is the part
/// with no oracle behind it — nothing detects a surface silently dropped from
/// [`described_surfaces`], and dropping one leaves every other test green.
/// Pinning the names turns that into a failing assertion.
const EXPECTED_SURFACES: &[&str] = &[
    "explain error --help",
    "info pipeline entry",
    "unknown-code advice",
    "unknown-name advice",
    "info error-sites hint",
    ".claude/CLAUDE.md",
    ".github/copilot-instructions.md",
];

#[test]
fn the_watched_surfaces_are_exactly_the_expected_roster() {
    let watched: Vec<&str> = described_surfaces()
        .iter()
        .map(|(surface, ..)| *surface)
        .collect();
    // A documentation surface this checkout does not carry is not expected
    // either; in the development repository both are present, so the roster
    // is the full one there.
    let present = present_claim_surfaces();
    let expected: Vec<&str> = EXPECTED_SURFACES
        .iter()
        .copied()
        .filter(|surface| !DOCUMENTED_CLAIM_SURFACES.contains(surface) || present.contains(surface))
        .collect();
    assert_eq!(
        watched, expected,
        "the set of watched surfaces changed. Adding one is good — add it here \
         too. Removing one needs a reason: `info error-sites hint` in \
         particular is what ADR 19.8.26b §1.2 rests on, the row proving the \
         overclaim had already crossed into a second namespace where no review \
         of `explain error` would have looked."
    );
}

#[test]
fn the_test_copy_of_the_withheld_set_agrees_with_production() {
    let mut production = error_catalogue::unlisted_kinds().to_vec();
    let mut beside_the_test: Vec<&str> = DELIBERATELY_UNLISTED
        .iter()
        .map(|(kind, _)| *kind)
        .collect();
    production.sort_unstable();
    beside_the_test.sort_unstable();
    assert_eq!(
        production, beside_the_test,
        "`DELIBERATELY_UNLISTED` carries a per-entry reason that `UNLISTED_KINDS` \
         does not, so the two lists stay separate — but they must name the same \
         kinds, or the prose check is keyed on a copy"
    );
}
