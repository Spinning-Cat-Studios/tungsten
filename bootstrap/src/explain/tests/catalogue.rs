//! Error-catalogue coverage and the code lookup (ADR 8.8.26a).
//!
//! The oracle here reads the real `ElabErrorKind::code()` arms rather than a
//! list of variant names typed beside it — see [`kind_code_pairs`].

use crate::explain::error_catalogue;

// ─────────────────────────────────────────────────────────────────────────────
// Error catalogue tests
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that every known error kind name has a corresponding explanation.
#[test]
fn all_error_kinds_have_explanations() {
    let names = error_catalogue::all_known_names();
    assert!(
        !names.is_empty(),
        "CATEGORIES should contain at least one error kind"
    );
    for name in &names {
        assert!(
            error_catalogue::get_explanation_by_name(name),
            "missing explanation for error kind: {}",
            name
        );
    }
}

/// Every code `ElabErrorKind::code()` produces must resolve to an explanation.
///
/// The oracle is the **real** `code()` arms, parsed out of
/// `elaborate/error/kind.rs` — not a list of variant names typed beside this
/// test. That is what the previous version did, and a hand-maintained list
/// cannot detect a variant missing from itself: 15 of 53 kinds had a code and a
/// rendered message but no explanation, and the "exhaustive" test was green
/// (ADR 8.8.26a).
///
/// Source-reading is deliberate and guarded: `code()` is an exhaustive match,
/// so it is the one place a new variant *must* appear, and the assertions below
/// fail loudly rather than skipping if the file cannot be read or yields
/// nothing.
#[test]
fn every_error_code_resolves() {
    let pairs = kind_code_pairs();
    for (kind, code) in &pairs {
        if DELIBERATELY_UNEXPLAINED.iter().any(|(k, _)| k == kind) {
            continue;
        }
        assert!(
            error_catalogue::resolve_query_for_test(code).is_some(),
            "code `{code}` (`{kind}`) does not resolve — every code a user can \
             see in compiler output must explain itself"
        );
        assert!(
            error_catalogue::resolve_query_for_test(kind).is_some(),
            "kind `{kind}` does not resolve by name"
        );
    }
}

/// Kinds with no explanation, each with the reason it has none.
///
/// An exclusion is a decision, so it is written down. Deleting an entry here
/// must make [`every_error_code_resolves`] fail — which is what
/// [`the_exclusion_list_is_load_bearing`] pins.
const DELIBERATELY_UNEXPLAINED: &[(&str, &str)] = &[(
    "Other",
    "the catch-all for messages with no structured kind; there is nothing \
     general to say about it",
)];

#[test]
fn the_exclusion_list_is_load_bearing() {
    for (kind, reason) in DELIBERATELY_UNEXPLAINED {
        assert!(
            error_catalogue::resolve_query_for_test(kind).is_none(),
            "`{kind}` is excluded as unexplainable ({reason}) but DOES resolve — \
             drop it from the exclusion list"
        );
    }
}

/// Kinds that resolve in `explain error <code|name>` but are deliberately
/// absent from the no-argument listing, each with the reason (ADR 15.8.26b).
///
/// Distinct from [`DELIBERATELY_UNEXPLAINED`]: these HAVE explanations — a
/// user holding the code can look it up — the listing just does not advertise
/// them among codes a user might encounter.
pub(super) const DELIBERATELY_UNLISTED: &[(&str, &str)] = &[(
    "InternalError",
    "the compiler's own broken invariant; not user-actionable, so the listing \
     does not advertise it",
)];

#[test]
fn unlisted_kinds_resolve_but_are_not_listed() {
    let listing = error_catalogue::render_error_list();
    let pairs = kind_code_pairs();
    for (kind, reason) in DELIBERATELY_UNLISTED {
        assert!(
            error_catalogue::resolve_query_for_test(kind).is_some(),
            "`{kind}` is unlisted ({reason}) but must still resolve by name"
        );
        let code = &pairs
            .iter()
            .find(|(k, _)| k == kind)
            .unwrap_or_else(|| panic!("`{kind}` is not a live ElabErrorKind variant"))
            .1;
        assert!(
            error_catalogue::resolve_query_for_test(code).is_some(),
            "`{kind}`'s code `{code}` must still resolve"
        );
        assert!(
            !listing.contains(code),
            "`{kind}`'s code `{code}` must NOT appear in the no-argument listing"
        );
    }
}

/// Every catalogued entry names a kind that still exists.
///
/// The other direction: a renamed or deleted variant leaves an entry that can
/// never be reached from a diagnostic.
#[test]
fn no_catalogue_entry_names_a_kind_that_no_longer_exists() {
    let pairs = kind_code_pairs();
    for name in error_catalogue::all_known_names() {
        assert!(
            pairs.iter().any(|(kind, _)| kind == name),
            "catalogue entry `{name}` is not a live ElabErrorKind variant"
        );
    }
}

/// Every explanation carries substance — an entry stubbed to `""` is worse than
/// no entry, because the coverage tests above would call it covered.
#[test]
fn every_explanation_has_a_detail_and_an_example() {
    for name in error_catalogue::all_known_names() {
        let (detail, example, code) = error_catalogue::entry_body_for_test(name)
            .unwrap_or_else(|| panic!("`{name}` is listed but has no explanation"));
        assert!(detail.len() > 40, "`{name}` detail is a stub: {detail:?}");
        assert!(!example.is_empty(), "`{name}` has no example");
        assert!(
            code.len() == 5 && code.starts_with(['E', 'W']),
            "`{name}` has a malformed code: {code:?}"
        );
    }
}

/// `(kind name, code)` for every arm of `ElabErrorKind::code()`.
fn kind_code_pairs() -> Vec<(String, String)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/elaborate/error/kind/codes.rs"
    );
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read the code() oracle at {path}: {e}"));

    let mut pairs = Vec::new();
    for line in src.lines() {
        let Some(rest) = line.trim().strip_prefix("ElabErrorKind::") else {
            continue;
        };
        let Some((lhs, rhs)) = rest.split_once("=>") else {
            continue;
        };
        let kind: String = lhs
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let code: String = rhs
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .into();
        if code.len() == 5 && code.starts_with(['E', 'W']) {
            pairs.push((kind, code));
        }
    }
    // Non-vacuity: a parse that silently matched nothing would make every
    // coverage assertion above pass over an empty set.
    assert!(
        pairs.len() >= 50,
        "the code() oracle parsed only {} arm(s) — the match shape changed",
        pairs.len()
    );
    pairs
}

// ─────────────────────────────────────────────────────────────────────────────
// Code lookup (ADR 8.8.26a)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_code_resolves_to_its_kind() {
    assert_eq!(
        error_catalogue::resolve_query_for_test("E0061"),
        Some("NonStrictlyPositive")
    );
    assert_eq!(
        error_catalogue::resolve_query_for_test("E0010"),
        Some("TypeMismatch")
    );
}

#[test]
fn a_code_resolves_case_insensitively() {
    // Users retype what they saw, and shells lowercase nothing for them.
    assert_eq!(
        error_catalogue::resolve_query_for_test("e0061"),
        Some("NonStrictlyPositive")
    );
    assert_eq!(
        error_catalogue::resolve_query_for_test("E0061"),
        error_catalogue::resolve_query_for_test("e0061")
    );
}

#[test]
fn the_kind_name_path_is_not_regressed() {
    assert_eq!(
        error_catalogue::resolve_query_for_test("NonStrictlyPositive"),
        Some("NonStrictlyPositive")
    );
    assert_eq!(
        error_catalogue::resolve_query_for_test("typemismatch"),
        Some("TypeMismatch")
    );
}

#[test]
fn a_query_that_is_neither_resolves_to_nothing() {
    assert_eq!(error_catalogue::resolve_query_for_test("Nonsense"), None);
    assert_eq!(error_catalogue::resolve_query_for_test(""), None);
}

#[test]
fn code_shaped_inputs_are_told_apart_from_names() {
    // The failure paths differ: a bad code gets no fuzzy name suggestion,
    // because "did you mean `TypeMismatch`?" is useless advice for `E0099`.
    for code in ["E0010", "e0010", "W0001", "E9999"] {
        assert!(
            error_catalogue::is_code_shaped_for_test(code),
            "`{code}` should read as a code"
        );
    }
    for name in ["TypeMismatch", "E001", "E00100", "EXXXX", "", "0E010"] {
        assert!(
            !error_catalogue::is_code_shaped_for_test(name),
            "`{name}` should NOT read as a code"
        );
    }
}

#[test]
fn an_unresolvable_code_still_fails() {
    // E0099 is not assigned. The command must fail, not fall back to a
    // near-miss name — that is the whole point of `is_code_shaped`.
    assert_eq!(error_catalogue::resolve_query_for_test("E0099"), None);
    assert!(error_catalogue::is_code_shaped_for_test("E0099"));
}

#[test]
fn the_listing_prints_every_code_beside_its_kind() {
    let listing = error_catalogue::render_error_list();
    // A reader arriving from `error[E0061]` scans for the code, not the name.
    for (kind, code) in kind_code_pairs() {
        if DELIBERATELY_UNEXPLAINED.iter().any(|(k, _)| *k == kind)
            || DELIBERATELY_UNLISTED.iter().any(|(k, _)| *k == kind)
        {
            continue;
        }
        assert!(
            listing.contains(&code),
            "code `{code}` (`{kind}`) is missing from the listing"
        );
        assert!(listing.contains(&kind), "kind `{kind}` is missing");
    }
    assert!(
        listing.contains("<code|name>"),
        "the footer must advertise that codes are accepted: {listing}"
    );
}
