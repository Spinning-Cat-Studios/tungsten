//! Tests: bootstrap/src/doctor/suggest_tools/patterns/termination.rs
//!
//! These exist because the patterns shipped without them (ADR 12.8.26a's
//! `/check-adr` pass). That mattered more than a usual coverage hole: the whole
//! reason the entries were added is that `suggest-tools` — the first step
//! `/action-adr` mandates — answered "No matching diagnostic tools found" for
//! the newest hard gate in the compiler. An untested pattern table can regress
//! straight back to that answer, and nothing else in the build would notice.
//!
//! The helpers come from the sibling `tests` module rather than being copied.

use super::tests::{assert_suggests, assert_top_suggestion};
use super::*;

/// The E0062 query must reach the census tool FIRST, not merely somewhere in
/// the list. Ranking is the property: `explain error E0062` is also a good
/// answer, so a table that led with the rule would still "suggest the right
/// things" while sending a reader to prose when they have a file in front of
/// them.
///
/// **What decides that order is declaration order in the table, NOT the
/// `relevance` figures** — see the note on the score cap in `mod.rs`. Verified
/// rather than assumed: dropping the census entry to `relevance: 0.50`, below
/// the rule's 0.90, leaves this test green and the shipped output unchanged,
/// while swapping the two entries' positions turns it red. So do not try to
/// re-rank these by editing a relevance value; move the entry.
#[test]
fn cannot_prove_termination_leads_with_the_census() {
    assert_top_suggestion("cannot prove termination", "doctor check type termination");
}

/// The proof-boundary query inverts that order, and the inversion is the point:
/// E0062 asks you to change the recursion, E0063 says an opt-out has reached
/// somewhere no annotation can fix — so the rule leads here.
#[test]
fn partial_in_proof_leads_with_the_rule_not_the_census() {
    assert_top_suggestion("partial in proof", "explain error E0063");
    assert_suggests("partial in proof", "doctor check type termination");
}

/// The two categories must stay distinct. `match_suggestions` merges every
/// matching pattern and deduplicates by command, so if the two tables were ever
/// folded into one both queries would answer with the same lead.
#[test]
fn the_two_categories_rank_the_same_tools_differently() {
    let recursion = match_suggestions("cannot prove termination");
    let proof = match_suggestions("partial in proof");
    assert_ne!(
        recursion[0].command, proof[0].command,
        "one table for two questions — recursion: {:?}, proof: {:?}",
        recursion[0].command, proof[0].command
    );
}

/// `--why-not-certified` must be recommended for an E0062 rejection. This is
/// the pairing ADR 12.8.26a §5.3 caught being blocked by the very gate it
/// explains: the recommendation and the command's reachability have to move
/// together, and `doctor tool-reachability` guards only the second half.
#[test]
fn an_e0062_rejection_recommends_the_per_parameter_view() {
    assert_suggests("cannot prove termination", "--why-not-certified");
}

/// Routing by the *reason* text, not the headline. The diagnostic prints its
/// reason under a headline that reads self-explanatory, so this is what a stuck
/// reader actually pastes — and it is the half of the keyword list most likely
/// to be trimmed as redundant by someone reading only the headline.
#[test]
fn the_reason_text_a_user_pastes_routes_as_well_as_the_headline() {
    for pasted in [
        "no parameter has an inductive type to descend on",
        "which is not a known strict subterm of l",
    ] {
        assert_suggests(pasted, "doctor check type termination");
    }
}

/// The regression this file exists for: these queries returned nothing at all
/// until ADR 11.8.26b's retrospective. `e0062` and `e0063` are matched by no
/// other pattern, so an empty result here means the table was lost rather than
/// merely reordered — a failure the ranking tests above would report as an
/// index panic instead of as the thing that happened.
#[test]
fn the_error_codes_are_not_silently_unmatched() {
    for code in ["e0062", "e0063"] {
        assert!(
            !match_suggestions(code).is_empty(),
            "`{code}` matched no pattern — suggest-tools is blind to it again"
        );
    }
}
