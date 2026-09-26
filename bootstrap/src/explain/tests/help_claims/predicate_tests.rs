//! What [`super::predicate`] decides, asserted case by case.
//!
//! Three groups: the quadrant (withheld set empty/non-empty x names an
//! exception or not, at both strictness levels), the scanner's own behaviour,
//! and the five pre-fix wordings ADR 15.8.26b left standing, pinned as red
//! fixtures. Split from `predicate.rs` to keep both under the size cap.

use super::predicate::*;

// ─────────────────────────────────────────────────────────────────────────────
// The quadrant, at both strictness levels
// ─────────────────────────────────────────────────────────────────────────────

/// A stand-in withheld set. Both the set and the level are arguments to the
/// predicate, so the quadrant needs no `const` mutation and no manual demo.
const A_WITHHELD_SET: &[(&str, &str)] = &[("InternalError", "E9998")];
const NOTHING_WITHHELD: &[(&str, &str)] = &[];

/// Qualified, and names the exception.
const NAMES_THE_EXCEPTION: &str = "Lists every user-facing code. E9998 is \
     deliberately absent from the listing.";
/// Qualified, names no exception.
const NAMES_NO_EXCEPTION: &str = "Lists every user-facing code.";

#[test]
fn explains_accepts_a_qualified_claim_that_names_the_exception() {
    assert!(claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Explains,
        NAMES_THE_EXCEPTION
    ));
}

#[test]
fn explains_rejects_a_withheld_entry_that_goes_unnamed() {
    // The lying combination: something IS withheld and the long help never
    // says so. This is the arm that keeps the explaining paragraph alive.
    assert!(!claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Explains,
        NAMES_NO_EXCEPTION
    ));
}

#[test]
fn explains_rejects_an_exception_that_no_longer_exists() {
    // The other lying combination: nothing is withheld and the help still
    // describes something as absent.
    assert!(!claim_is_consistent(
        NOTHING_WITHHELD,
        ClaimStrictness::Explains,
        NAMES_THE_EXCEPTION
    ));
}

#[test]
fn explains_accepts_an_unqualified_claim_once_nothing_is_withheld() {
    // "every code" is simply true again, so the paydown is not blocked.
    assert!(claim_is_consistent(
        NOTHING_WITHHELD,
        ClaimStrictness::Explains,
        "Lists every code."
    ));
}

#[test]
fn qualifies_does_not_require_the_name_in_either_direction() {
    // A one-line hint has no room to name an exception, so the naming axis is
    // inert here — that difference is the design, not a loophole.
    assert!(claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Qualifies,
        NAMES_NO_EXCEPTION
    ));
    assert!(claim_is_consistent(
        NOTHING_WITHHELD,
        ClaimStrictness::Qualifies,
        NAMES_THE_EXCEPTION
    ));
}

#[test]
fn qualifies_still_requires_the_qualifier() {
    assert!(!claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Qualifies,
        "Run `tungsten explain error` to list every code."
    ));
}

#[test]
fn an_unqualified_claim_is_fine_when_nothing_is_withheld() {
    assert!(claim_is_consistent(
        NOTHING_WITHHELD,
        ClaimStrictness::Qualifies,
        "Run `tungsten explain error` to list every code."
    ));
}

#[test]
fn text_making_no_claim_at_all_passes_the_qualifier_arm() {
    assert!(claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Qualifies,
        "Show self-hosted-compiler error codes instead of bootstrap codes."
    ));
}

#[test]
fn one_unqualified_claim_condemns_a_text_that_also_carries_a_qualified_one() {
    // Rows 1 and 2 arrive in a single rendered help, so a per-text verdict has
    // to be over EVERY claim in it, not the first one found.
    assert!(!claim_is_consistent(
        A_WITHHELD_SET,
        ClaimStrictness::Explains,
        "Lists every user-facing code. E9998 is unlisted. Omit to list all.",
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// The scanner itself
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_scanner_reads_the_qualifier_through_a_line_wrap() {
    // clap wraps to terminal width; without normalization this is two claims
    // away from where the wrap happens to fall.
    assert_eq!(
        exhaustiveness_claims(&normalize_whitespace("lists every\n   user-facing code")),
        vec![true]
    );
}

#[test]
fn the_scanner_finds_a_claim_with_its_noun_elided() {
    assert_eq!(exhaustiveness_claims("Omit to list all."), vec![false]);
}

#[test]
fn the_scanner_ignores_a_totality_word_that_quantifies_nothing_listed() {
    assert!(exhaustiveness_claims("this applies to all users").is_empty());
    assert!(exhaustiveness_claims("every argument is optional").is_empty());
}

#[test]
fn the_scanner_reaches_a_noun_three_words_out() {
    assert_eq!(
        exhaustiveness_claims("to list all error kinds"),
        vec![false]
    );
}

#[test]
fn a_bare_mention_of_a_code_is_not_a_named_exception() {
    // `E9998` in an example line says nothing about what the listing omits.
    assert!(!names_a_withheld_entry(
        "tungsten explain error E9998",
        A_WITHHELD_SET
    ));
    assert!(!describes_an_exception("tungsten explain error E9998"));
}

#[test]
fn an_exclusion_marker_with_no_code_names_no_exception() {
    assert!(!describes_an_exception("some kinds are unlisted"));
}

#[test]
fn the_kind_name_is_an_accepted_spelling_of_the_exception() {
    assert!(names_a_withheld_entry(
        "InternalError is absent from the listing",
        A_WITHHELD_SET
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// The five pre-fix wordings, pinned as red fixtures
// ─────────────────────────────────────────────────────────────────────────────
//
// ADR 15.8.26b withheld E9998 and left these standing (ADR 19.8.26b §1.2).
// Pinning them is what replaces a one-off "revert it and watch it go red"
// demonstration with a standing regression over known-bad input.

/// Row 1 — `explain error`'s `long_about`, before the fix.
const PRE_FIX_LONG_ABOUT: &str = "\
With no argument, lists every code, name and summary, grouped by category. \
With an argument, prints a detailed explanation with examples. Accepts the \
CODE the compiler printed (E0010) or the name (TypeMismatch), \
case-insensitively — the code is the identifier that appears in diagnostic \
output, so it is the one to reach for.";

/// Row 2 — the `[KIND]` argument's help, before the fix.
const PRE_FIX_KIND_ARG_HELP: &str =
    "Error code (e.g., E0010) or kind name (e.g., \"TypeMismatch\"), \
     case-insensitive. Omit to list all.";

/// Row 3 — the unknown-**code** failure path, before the fix.
const PRE_FIX_UNKNOWN_CODE_ADVICE: &str =
    "Run `tungsten explain error` to list every code and kind.";

/// Row 4 — the unknown-**name** failure path, before the fix.
const PRE_FIX_UNKNOWN_NAME_ADVICE: &str = "Run `tungsten explain error` to list all error kinds.";

/// Row 5 — a **different** command's hint, before the fix.
const PRE_FIX_ERROR_SITES_HINT: &str = "hint: `tungsten explain error` lists every code";

#[test]
fn every_pre_fix_wording_is_rejected() {
    let long_help = [
        ("long_about", PRE_FIX_LONG_ABOUT),
        ("[KIND] arg help", PRE_FIX_KIND_ARG_HELP),
    ];
    for (row, text) in long_help {
        assert!(
            !claim_is_consistent(A_WITHHELD_SET, ClaimStrictness::Explains, text),
            "the pre-fix `{row}` must be rejected: {text}"
        );
    }
    let hints = [
        ("unknown-code advice", PRE_FIX_UNKNOWN_CODE_ADVICE),
        ("unknown-name advice", PRE_FIX_UNKNOWN_NAME_ADVICE),
        ("info error-sites hint", PRE_FIX_ERROR_SITES_HINT),
    ];
    for (row, text) in hints {
        assert!(
            !claim_is_consistent(A_WITHHELD_SET, ClaimStrictness::Qualifies, text),
            "the pre-fix `{row}` must be rejected: {text}"
        );
    }
}
