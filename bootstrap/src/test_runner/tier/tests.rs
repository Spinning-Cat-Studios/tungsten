//! Tests for cost-tier resolution and D7's guards.
//!
//! The two guards are asserted by *inducing the condition and expecting the
//! failure*, not by inspection — a guard nobody has watched fire is a guard
//! nobody knows works.

use super::*;

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

#[test]
fn a_tier_3_declaration_resolves_to_elaborate_only() {
    assert_eq!(
        decide_tier("src/compiler/test_list_ops.tg", Some(3), true, false),
        Ok(Some(CostTier::ElaborateOnly))
    );
}

#[test]
fn a_tier_5_declaration_resolves_to_run_body() {
    assert_eq!(
        decide_tier("src/compiler/test_strmap.tg", Some(5), true, true),
        Ok(Some(CostTier::RunBody))
    );
}

/// A file nothing requires and nothing declares is simply not governed — the
/// runner keeps its historical behaviour, so `tungsten test` still works on an
/// ad-hoc file outside the corpus.
#[test]
fn an_unrequired_undeclared_file_is_not_governed() {
    assert_eq!(decide_tier("scratch/probe.tg", None, false, true), Ok(None));
}

/// Guard (c). A default here is what the guard exists to refuse: tier 3 would
/// silently skip every new file, tier 5 would quietly re-open cause B.
#[test]
fn guard_c_a_required_but_undeclared_file_is_an_error() {
    assert_eq!(
        decide_tier("src/compiler/test_brand_new.tg", None, true, false),
        Err(TierError::Undeclared {
            key: "src/compiler/test_brand_new.tg".to_string()
        })
    );
}

/// Guard (a). `test_string_concat.tg` was the live instance: declared
/// `--check-only` while making 14 runtime `assert_eq_string` calls, so the
/// manifest would have converted its five cause-A findings into `Skipped` and
/// P1's fix for them would never have run under the gate.
#[test]
fn guard_a_a_tier_3_declaration_on_a_file_that_asserts_at_runtime_is_an_error() {
    assert_eq!(
        decide_tier("src/compiler/test_string_concat.tg", Some(3), true, true),
        Err(TierError::AssertsAtRuntimeButDeclaredTier3 {
            key: "src/compiler/test_string_concat.tg".to_string()
        })
    );
}

/// Guard (a) must not fire on a tier-5 file — otherwise every runtime suite
/// would be an error and the guard would be indistinguishable from a ban.
#[test]
fn guard_a_does_not_fire_on_a_tier_5_file_that_asserts() {
    assert_eq!(
        decide_tier("src/compiler/test_ctor_store.tg", Some(5), true, true),
        Ok(Some(CostTier::RunBody))
    );
}

#[test]
fn a_tier_outside_the_cost_scale_is_an_error() {
    assert_eq!(
        decide_tier("src/compiler/test_x.tg", Some(4), true, false),
        Err(TierError::UnknownTier {
            key: "src/compiler/test_x.tg".to_string(),
            number: 4
        })
    );
}

#[test]
fn the_tier_numbers_round_trip() {
    assert_eq!(CostTier::ElaborateOnly.number(), 3);
    assert_eq!(CostTier::RunBody.number(), 5);
    assert_eq!(CostTier::from_number(3), Some(CostTier::ElaborateOnly));
    assert_eq!(CostTier::from_number(5), Some(CostTier::RunBody));
    assert_eq!(CostTier::from_number(0), None);
}

/// Every error must say what to do about it. A guard whose message is just
/// "error" is a guard whose reader disables it.
#[test]
fn every_error_names_the_file_and_the_remedy() {
    let undeclared = TierError::Undeclared {
        key: "src/compiler/test_new.tg".to_string(),
    }
    .to_string();
    assert!(undeclared.contains("src/compiler/test_new.tg"));
    assert!(undeclared.contains(MANIFEST_FILENAME));

    let mis_tiered = TierError::AssertsAtRuntimeButDeclaredTier3 {
        key: "src/compiler/test_string_concat.tg".to_string(),
    }
    .to_string();
    assert!(mis_tiered.contains("test_string_concat.tg"));
    assert!(mis_tiered.contains("tier 5"));

    let unknown = TierError::UnknownTier {
        key: "a.tg".to_string(),
        number: 4,
    }
    .to_string();
    assert!(unknown.contains('4'));

    let unreadable = TierError::Unreadable {
        path: PathBuf::from("/x/tg-test-tiers.toml"),
        detail: "expected `=`".to_string(),
    }
    .to_string();
    assert!(unreadable.contains("expected `=`"));
}

// ---------------------------------------------------------------------------
// Flag ∨ declaration
// ---------------------------------------------------------------------------

/// A tier-3 declaration alone skips the bodies — the behaviour that makes the
/// gate and the per-file target agree without the flag being written twice.
#[test]
fn a_tier_3_declaration_skips_bodies_with_no_flag() {
    assert!(should_skip_bodies(false, Some(CostTier::ElaborateOnly)));
}

/// The explicit flag still wins on its own, so an ad-hoc cost-3 run of an
/// undeclared file needs no manifest entry.
#[test]
fn the_explicit_flag_still_skips_bodies_on_its_own() {
    assert!(should_skip_bodies(true, None));
    assert!(should_skip_bodies(true, Some(CostTier::RunBody)));
}

/// Tier 5 and no flag runs the bodies — the case cause B was breaking, and the
/// polarity without which the two tests above would pass vacuously.
#[test]
fn tier_5_with_no_flag_runs_the_bodies() {
    assert!(!should_skip_bodies(false, Some(CostTier::RunBody)));
    assert!(!should_skip_bodies(false, None));
}

// ---------------------------------------------------------------------------
// Manifest lookups, driven from literal TOML
// ---------------------------------------------------------------------------

/// A manifest parsed from `text`, rooted at `/repo` — no filesystem involved,
/// so the lookups below are assertable over literal input.
fn manifest(text: &str) -> TierManifest {
    TierManifest::parse(Path::new("/repo/tg-test-tiers.toml"), text).expect("fixture must parse")
}

const TWO_BLOCK_MANIFEST: &str = r#"
must_declare = ["src/compiler/test_*.tg"]

[files]
"src/compiler/test_a.tg" = { tier = 5, why = "x" }

[[expected_failure]]
file = "src/compiler/test_a.tg"
tests = ["test_one", "test_two"]
adr = "7.8.26c"
why = "y"

[[expected_failure]]
file = "src/compiler/test_b.tg"
tests = ["test_three"]
adr = "9.9.99z"
why = "z"
"#;

/// Each block's tests map to *its own* owning ADR, and a file with no block
/// gets nothing — the filter is what keeps one file's permission from
/// un-gating another's.
#[test]
fn expected_failures_are_scoped_to_their_own_file_and_carry_their_own_adr() {
    let manifest = manifest(TWO_BLOCK_MANIFEST);

    let a = manifest.expected_failures_for_key("src/compiler/test_a.tg");
    assert_eq!(a.len(), 2);
    assert_eq!(a.get("test_one").map(String::as_str), Some("7.8.26c"));
    assert_eq!(a.get("test_two").map(String::as_str), Some("7.8.26c"));
    assert!(
        !a.contains_key("test_three"),
        "another file's block must not leak into this one"
    );

    let b = manifest.expected_failures_for_key("src/compiler/test_b.tg");
    assert_eq!(b.get("test_three").map(String::as_str), Some("9.9.99z"));

    assert!(
        manifest
            .expected_failures_for_key("src/compiler/test_unlisted.tg")
            .is_empty(),
        "a file with no block must have no permissions"
    );
}

/// A manifest with no `[[expected_failure]]` blocks yields none — the normal
/// state, and the one an empty-by-default implementation would fake.
#[test]
fn a_manifest_without_expected_failures_yields_none() {
    let manifest = manifest("must_declare = []\n[files]\n");
    assert!(manifest
        .expected_failures_for_key("src/compiler/test_a.tg")
        .is_empty());
}

/// `must_declare` discriminates: it must answer true for a match and false for
/// a non-match, or guard (c) either fires on everything or on nothing.
#[test]
fn must_declare_matches_only_the_declared_globs() {
    let manifest = manifest(TWO_BLOCK_MANIFEST);
    assert!(manifest.matches_must_declare("src/compiler/test_a.tg"));
    assert!(manifest.matches_must_declare("src/compiler/test_anything.tg"));
    assert!(!manifest.matches_must_declare("src/compiler/mustfail_ast_compare.tg"));
    assert!(!manifest.matches_must_declare("tests/try_block.tg"));
}

/// An empty `must_declare` requires nothing — so a project that declares no
/// globs is not accidentally forced to enumerate every file.
#[test]
fn an_empty_must_declare_requires_nothing() {
    let manifest = manifest("must_declare = []\n[files]\n");
    assert!(!manifest.matches_must_declare("src/compiler/test_a.tg"));
}

/// Unparseable TOML is an `Unreadable` error carrying the parser's complaint,
/// not a silent empty manifest — which would disable both guards at once.
#[test]
fn a_malformed_manifest_is_an_error_not_an_empty_one() {
    let broken = TierManifest::parse(Path::new("/repo/tg-test-tiers.toml"), "must_declare = [");
    assert!(matches!(broken, Err(TierError::Unreadable { .. })));
}
