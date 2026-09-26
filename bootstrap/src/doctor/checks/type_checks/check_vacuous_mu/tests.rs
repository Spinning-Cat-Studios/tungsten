//! Tests for the vacuous-μ census.
//!
//! Over injected `Type` values rather than elaborated projects: the predicate
//! is the whole check, and a test that boots an elaborator to assert it would
//! be slower and prove less.

use std::collections::HashMap;

use tungsten_core::Type;

use super::{render, vacuous_binder, vacuous_types, VacuousType};

fn mu(var: &str, body: Type) -> Type {
    Type::mu(var, body)
}

fn tyvar(name: &str) -> Type {
    Type::TyVar(name.to_string())
}

// ============================================================================
// The predicate
// ============================================================================

/// The exact shape a nested inductive family encodes to (ADR 11.8.26c): the
/// binder's body is the binder.
#[test]
fn a_bare_self_referential_binder_is_vacuous() {
    assert_eq!(
        vacuous_binder("Rose", &mu("α_Rose", tyvar("α_Rose"))),
        Some("α_Rose".to_string())
    );
}

/// Non-vacuity for the whole check: an ordinary recursive ADT encodes a real
/// `Sum` under its binder and must NOT be reported. Without this, "flag every
/// μ" would satisfy the test above.
#[test]
fn an_ordinary_recursive_adt_is_not_vacuous() {
    let list = mu("α_List", Type::sum(Type::Unit, tyvar("α_List")));
    assert_eq!(vacuous_binder("List", &list), None);
}

/// A mutual group's chain is longer than one binder, and its body is
/// structural — the check must peel the whole chain before judging, not stop
/// at the first binder.
#[test]
fn a_mutual_recursion_chain_with_a_structural_body_is_not_vacuous() {
    let a = mu("α_A", mu("α_B", Type::sum(Type::Unit, tyvar("α_B"))));
    assert_eq!(vacuous_binder("A", &a), None);
}

/// The self-reference is found however deep the chain is.
#[test]
fn a_vacuous_body_is_found_through_a_whole_binder_chain() {
    let nested = mu("α_A", mu("α_B", tyvar("α_A")));
    assert_eq!(vacuous_binder("A", &nested), Some("α_A".to_string()));
}

/// **The false positive this check shipped with, as a regression test.**
///
/// `type RoseKids = NoKids | Kid(Rose, RoseKids)` / `type Rose = Node(RoseKids)`
/// is the *documented workaround* for E0064 — it compiles, and matching on it
/// works. Yet `Rose` encodes to `μα_RoseKids. α_RoseKids`: a chain binder sits
/// in body position, because the ADR 18.4.26i group encoding makes an inner
/// binder a MARKER for a sibling member rather than a type.
///
/// The first draft tested "the body is one of the chain's binders" and flagged
/// this file. A guard that fires on the very code the diagnostic recommends is
/// worse than no guard.
#[test]
fn a_mutual_group_marker_in_body_position_is_not_vacuous() {
    let rose = mu("α_RoseKids", tyvar("α_RoseKids"));
    assert_eq!(
        vacuous_binder("Rose", &rose),
        None,
        "α_RoseKids marks a sibling that HAS a body; only self-reference is vacuous"
    );
    // And the sibling itself, whose encoding genuinely is self-referential in
    // name only, is judged on its own name — not Rose's.
    assert_eq!(
        vacuous_binder("RoseKids", &rose),
        Some("α_RoseKids".to_string())
    );
}

/// The bare (un-prefixed) spelling of the binder counts too — `Display` strips
/// `α_`, and encodings are not guaranteed to carry the prefix.
#[test]
fn a_binder_without_the_alpha_prefix_still_matches_its_type() {
    assert_eq!(
        vacuous_binder("Rose", &mu("Rose", tyvar("Rose"))),
        Some("Rose".to_string())
    );
}

/// A μ whose body is a *free* variable — not one it bound — is not vacuous in
/// this sense. It is a different defect (an unresolved reference), and
/// reporting it here would send the reader to the wrong fix.
#[test]
fn a_body_naming_a_different_type_is_not_vacuous() {
    assert_eq!(vacuous_binder("A", &mu("α_A", tyvar("α_Elsewhere"))), None);
}

/// Non-μ types are not candidates at all.
#[test]
fn a_type_with_no_binder_is_not_vacuous() {
    assert_eq!(vacuous_binder("Nat", &Type::Nat), None);
    assert_eq!(vacuous_binder("S", &Type::sum(Type::Unit, Type::Nat)), None);
    // A bare TyVar is a reference, not an encoding with a binder to be vacuous.
    assert_eq!(vacuous_binder("Rose", &tyvar("α_Rose")), None);
}

// ============================================================================
// The census
// ============================================================================

#[test]
fn the_census_reports_only_the_vacuous_types_sorted() {
    let mut encodings: HashMap<String, Type> = HashMap::new();
    encodings.insert("Rose".to_string(), mu("α_Rose", tyvar("α_Rose")));
    encodings.insert("Apple".to_string(), mu("α_Apple", tyvar("α_Apple")));
    encodings.insert(
        "List".to_string(),
        mu("α_List", Type::sum(Type::Unit, tyvar("α_List"))),
    );
    encodings.insert("Pair".to_string(), Type::product(Type::Nat, Type::Nat));
    // The workaround shape: a marker for a sibling, and NOT a finding.
    encodings.insert(
        "Cherry".to_string(),
        mu("α_CherryKids", tyvar("α_CherryKids")),
    );

    assert_eq!(
        vacuous_types(encodings.iter()),
        vec![
            VacuousType {
                name: "Apple".to_string(),
                binder: "α_Apple".to_string(),
            },
            VacuousType {
                name: "Rose".to_string(),
                binder: "α_Rose".to_string(),
            },
        ],
        "sorted by name, so the report is stable across runs"
    );
}

#[test]
fn a_project_with_no_vacuous_encodings_yields_an_empty_census() {
    let encodings: HashMap<String, Type> = HashMap::from([(
        "List".to_string(),
        mu("α_List", Type::sum(Type::Unit, tyvar("α_List"))),
    )]);
    assert_eq!(vacuous_types(encodings.iter()), vec![]);
}

// ============================================================================
// The report
// ============================================================================

/// The clean report states how many encodings it actually looked at. "✓" over
/// zero types and "✓" over two hundred read identically otherwise — the same
/// `0 violations` vs `0 inputs` distinction `code-health`'s reach line makes.
#[test]
fn the_clean_report_states_how_many_it_checked() {
    let (text, clean) = render(&[], 41, "main.tg");
    assert!(clean);
    assert!(text.contains("41"), "{text}");
    assert!(text.starts_with('✓'), "{text}");
    assert!(
        !text.contains("NOTHING WAS EXAMINED"),
        "a real census must not carry the empty-input warning: {text}"
    );
}

/// A census over zero encodings says so in words. Legitimate (a file of only
/// parameterized types caches none), so not an error — but a bare "✓" over
/// nothing is how a check gets trusted for work it never did, which is the
/// same defect the `mu-unfold-exemptions` default shipped with.
#[test]
fn a_census_over_zero_encodings_says_nothing_was_examined() {
    let (text, clean) = render(&[], 0, "generic_only.tg");
    assert!(clean, "zero encodings is legitimate, not a failure");
    assert!(text.contains("NOTHING WAS EXAMINED"), "{text}");
    assert!(
        text.contains("parameterized"),
        "and says why that can be fine: {text}"
    );
}

/// The failure names each type, its binder, and the fix — a census that only
/// counted would send the reader back to the tool that E0064 blocks.
#[test]
fn the_failure_report_names_each_type_its_binder_and_the_fix() {
    let found = vec![VacuousType {
        name: "Rose".to_string(),
        binder: "α_Rose".to_string(),
    }];
    let (text, clean) = render(&found, 3, "rose.tg");

    assert!(!clean);
    assert!(text.contains("Rose"), "{text}");
    assert!(text.contains("α_Rose"), "{text}");
    assert!(text.contains("E0064"), "{text}");
    assert!(
        text.contains("non-generic intermediate type"),
        "the report must carry the fix: {text}"
    );
    assert!(
        text.contains("1 of 3"),
        "the report must say how many of how many: {text}"
    );
}
