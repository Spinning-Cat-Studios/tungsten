//! The self-hosted error catalogue's lookup, rendering and range discipline.
//!
//! Split from `mod.rs` (ADR 19.8.26d review): the catalogue grows one entry per
//! self-hosted error code, so the file was heading for its size threshold with
//! the tests riding along. They assert what a *reader* of `explain error
//! --self-hosted` gets, which is a different question from how the lookup is
//! implemented.

use super::*;

#[test]
fn lookup_by_code() {
    let entry = SELF_HOSTED_ERRORS.iter().find(|e| e.code == "E0001");
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().name, "ErrTypeMismatch");
}

#[test]
fn lookup_by_name() {
    let entry = SELF_HOSTED_ERRORS
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("ErrUnresolvedValue"));
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().code, "E0101");
}

#[test]
fn the_listing_carries_its_header_body_and_footer() {
    let listing = self_hosted_error_list();
    assert!(listing.starts_with("Self-Hosted Compiler Error Reference\n"));
    assert!(listing.contains("See `tungsten explain error` for bootstrap codes."));
    assert!(listing.contains(&render_error_list_body()), "body missing");
    assert!(listing.ends_with("to look up by name.\n"), "footer missing");
}

#[test]
fn listing_body_groups_every_entry_under_its_own_category() {
    let body = render_error_list_body();
    assert!(!body.is_empty(), "the listing body must not be empty");
    // Every catalogue entry appears exactly once, under a heading — a
    // dropped category or a mis-filtered group would lose entries silently.
    for entry in SELF_HOSTED_ERRORS {
        assert_eq!(
            body.matches(entry.code).count(),
            1,
            "{} listed {} time(s)",
            entry.code,
            body.matches(entry.code).count()
        );
        assert!(body.contains(entry.name), "{} missing", entry.name);
    }
    // A heading is present exactly for the categories that have entries.
    for cat in CATEGORY_ORDER {
        let used = SELF_HOSTED_ERRORS.iter().any(|e| e.category == *cat);
        assert_eq!(
            body.contains(&format!("{cat}:\n")),
            used,
            "heading `{cat}` present={}, used={used}",
            body.contains(&format!("{cat}:\n"))
        );
    }
}

#[test]
fn short_desc_keeps_the_first_sentence_only() {
    // The listing shows one line per code, so a multi-sentence description
    // is cut at the first period — the period and everything after it go.
    assert_eq!(short_desc("First. Second. Third."), "First");
    // No period: the whole string survives unchanged.
    assert_eq!(short_desc("no trailing period"), "no trailing period");
    assert_eq!(short_desc(""), "");
    // A leading period yields an empty summary rather than panicking.
    assert_eq!(short_desc(".leading"), "");
}

#[test]
fn listing_rows_show_each_entrys_first_sentence() {
    // Ties the rendered listing to short_desc: every row must carry the
    // real summary, not a placeholder or an empty string.
    let body = render_error_list_body();
    for entry in SELF_HOSTED_ERRORS {
        let summary = short_desc(entry.description);
        assert!(!summary.is_empty(), "{} has an empty summary", entry.code);
        assert!(
            body.contains(summary),
            "{} row is missing its summary {summary:?}",
            entry.code
        );
    }
}

#[test]
fn listing_body_follows_the_declared_category_order() {
    let body = render_error_list_body();
    let mut last = 0usize;
    for cat in CATEGORY_ORDER {
        if let Some(at) = body.find(&format!("{cat}:\n")) {
            assert!(at >= last, "category `{cat}` is out of declared order");
            last = at;
        }
    }
}

#[test]
fn bootstrap_crossref_exists() {
    assert_eq!(bootstrap_equivalent("E0001"), Some("TypeMismatch"));
    assert_eq!(bootstrap_equivalent("E0101"), Some("UndefinedVariable"));
    assert_eq!(bootstrap_equivalent("E0999"), None);
}

#[test]
fn all_entries_have_unique_codes() {
    let mut codes: Vec<&str> = SELF_HOSTED_ERRORS.iter().map(|e| e.code).collect();
    codes.sort();
    codes.dedup();
    assert_eq!(codes.len(), SELF_HOSTED_ERRORS.len());
}

/// AC9. `explain error --self-hosted` resolves the mirror's code, by code
/// and by kind name — the failure this catches is a diagnostic the
/// compiler emits and its own explainer calls unknown.
#[test]
fn the_positivity_code_resolves_by_code_and_by_name() {
    assert_eq!(
        lookup("E0700").map(|e| e.name),
        Some("ErrNonStrictlyPositive")
    );
    assert_eq!(
        lookup("e0700").map(|e| e.name),
        Some("ErrNonStrictlyPositive")
    );
    assert_eq!(
        lookup("ErrNonStrictlyPositive").map(|e| e.code),
        Some("E0700")
    );
    assert_eq!(
        lookup("errnonstrictlypositive").map(|e| e.code),
        Some("E0700")
    );
}

/// The termination mirror claimed the two codes E0710 reserved for it
/// (ADR 19.8.26d D3), and they resolve **separately**: a proof reaching a
/// partial constant and a recursion that does not descend are different
/// repairs, so a lookup that answered both with one entry would be wrong
/// in the half a reader is more likely to arrive with.
#[test]
fn both_termination_codes_resolve_to_their_own_entry() {
    assert_eq!(
        lookup("E0710").map(|e| e.name),
        Some("ErrCannotProveTermination")
    );
    assert_eq!(lookup("E0711").map(|e| e.name), Some("ErrPartialInProof"));
    assert_eq!(
        lookup("ErrPartialInProof").map(|e| e.code),
        Some("E0711"),
        "by name as well as by code"
    );
}

/// The rest of E0700-E0799 is RESERVED, not catalogued: a reserved code
/// resolves to nothing rather than to its neighbour, so a later gate
/// claiming one finds it free rather than already answered.
#[test]
fn a_reserved_soundness_code_resolves_to_nothing() {
    assert_eq!(lookup("E0705").map(|e| e.code), None);
    assert_eq!(lookup("E0712").map(|e| e.code), None);
}

#[test]
fn fuzzy_finds_partial_code() {
    assert_eq!(fuzzy_match_self_hosted("E000"), Some("E0001"));
}

#[test]
fn fuzzy_finds_partial_name() {
    assert_eq!(fuzzy_match_self_hosted("mismatch"), Some("ErrTypeMismatch"));
}
