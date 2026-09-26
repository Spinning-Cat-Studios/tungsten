//! Invariants that hold across the whole pattern registry, rather than for any
//! one category (split from [`super::tests`], ADR 13.8.26c review).
//!
//! These are the tests that fail when the *matcher* breaks — scoring, ordering,
//! case folding, the relevance cap, the cost-tier contract — as opposed to when
//! one category's keywords stop matching the message they were drawn from.

use super::tests::{assert_suggests, assert_top_suggestion};
use super::*;

// ── Edge cases ──────────────────────────────────────────────────

#[test]
fn test_empty_query_returns_empty() {
    let results = match_suggestions("");
    assert!(results.is_empty());
}

#[test]
fn test_unrelated_query_returns_empty() {
    let results = match_suggestions("how do I write a hello world program");
    assert!(results.is_empty());
}

#[test]
fn test_json_output_is_valid() {
    let results = match_suggestions("SIGSEGV");
    let json = serde_json::to_string(&results).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(!parsed.is_empty());
    // Verify required fields
    let first = &parsed[0];
    assert!(first.get("command").is_some());
    assert!(first.get("cost").is_some());
    assert!(first.get("reason").is_some());
    assert!(first.get("relevance").is_some());
}

#[test]
fn test_multi_keyword_match_boosts_relevance() {
    // "mutual recursion cycle" matches both "mutual recursion" and "cycle"
    // keywords, so mutual-recursion-groups should score higher
    let results = match_suggestions("mutual recursion cycle in types");
    assert!(!results.is_empty());
    assert!(
        results[0].command.contains("mutual-recursion-groups"),
        "Expected mutual-recursion-groups as top result for multi-keyword match, got '{}'",
        results[0].command
    );
}

#[test]
fn test_all_categories_have_suggestions() {
    // Verify each category is reachable
    let test_cases = [
        "sigsegv",
        "type mismatch",
        "stack overflow",
        "infinite loop",
        "elaboration error",
        "encoding",
        "mutual recursion",
        "cross-file",
    ];
    for query in &test_cases {
        let results = match_suggestions(query);
        assert!(
            !results.is_empty(),
            "Category '{}' returned no suggestions",
            query
        );
    }
}

#[test]
fn test_suggestions_sorted_by_relevance() {
    let results = match_suggestions("SIGSEGV");
    for window in results.windows(2) {
        assert!(
            window[0].relevance >= window[1].relevance,
            "Suggestions not sorted: {} ({}) came before {} ({})",
            window[0].command,
            window[0].relevance,
            window[1].command,
            window[1].relevance,
        );
    }
}

#[test]
fn test_no_duplicate_commands() {
    let results = match_suggestions("mutual recursion encoding cycle μ-type");
    let mut commands: Vec<&str> = results.iter().map(|s| s.command).collect();
    let len_before = commands.len();
    commands.sort();
    commands.dedup();
    assert_eq!(
        len_before,
        commands.len(),
        "Duplicate commands in suggestions"
    );
}

// ── Case insensitivity ──────────────────────────────────────────

#[test]
fn test_cross_file_suggests_error_enrichment() {
    let results = match_suggestions("cross-file error in different file");
    assert!(
        results
            .iter()
            .any(|s| s.command.contains("error-enrichment")),
        "Expected error-enrichment for cross-file query"
    );
}

#[test]
fn test_case_insensitive_matching() {
    let upper = match_suggestions("SIGSEGV");
    let lower = match_suggestions("sigsegv");
    let mixed = match_suggestions("SigSegV");

    assert_eq!(upper.len(), lower.len());
    assert_eq!(upper.len(), mixed.len());

    for (u, l) in upper.iter().zip(lower.iter()) {
        assert_eq!(u.command, l.command);
        assert_eq!(u.relevance, l.relevance);
    }
    for (u, m) in upper.iter().zip(mixed.iter()) {
        assert_eq!(u.command, m.command);
        assert_eq!(u.relevance, m.relevance);
    }
}

// ── Keyword in verbose error message ────────────────────────────

#[test]
fn test_keyword_in_verbose_error_message() {
    // Keywords buried in a long, noisy error message should still match
    let verbose = "Error at line 42 in module Foo: the compiler encountered \
                   a segmentation fault (SIGSEGV) while attempting to lower \
                   the ADT constructor for type Bar. Please report this bug.";
    let results = match_suggestions(verbose);
    assert!(
        results
            .iter()
            .any(|s| s.command.contains("check fold-consistency")),
        "Expected check-fold-consistency for verbose SIGSEGV message, got: {:?}",
        results.iter().map(|s| s.command).collect::<Vec<_>>()
    );
}

// ── Cross-category matching ─────────────────────────────────────

#[test]
fn test_cross_category_returns_suggestions_from_both() {
    // A query mentioning keywords from two categories should return
    // suggestions from both
    let results = match_suggestions("mutual recursion caused SIGSEGV");
    let has_segfault_tool = results
        .iter()
        .any(|s| s.command.contains("check fold-consistency"));
    let has_mutual_tool = results
        .iter()
        .any(|s| s.command.contains("mutual-recursion-groups"));
    assert!(
        has_segfault_tool && has_mutual_tool,
        "Expected suggestions from both segfault and mutual recursion categories, got: {:?}",
        results.iter().map(|s| s.command).collect::<Vec<_>>()
    );
}

// ── Relevance cap ───────────────────────────────────────────────

#[test]
fn test_relevance_never_exceeds_one() {
    // Even with many keyword hits, relevance should be capped at 1.0
    let heavy_query = "mutual recursion mutually recursive scc cycle \
                       circular type circular dependency encoding μ-type \
                       mu type mu_var tyvar alpha_ mu binder recursive type";
    let results = match_suggestions(heavy_query);
    for s in &results {
        assert!(
            s.relevance <= 1.0,
            "Relevance {} > 1.0 for command '{}'",
            s.relevance,
            s.command
        );
    }
}

// ── Cost field validity ─────────────────────────────────────────

#[test]
fn test_all_costs_in_valid_range() {
    // Every suggestion across all categories should have cost 1–5
    let queries = [
        "sigsegv",
        "type mismatch",
        "stack overflow",
        "infinite loop",
        "elaboration error",
        "encoding",
        "mutual recursion",
    ];
    for query in &queries {
        let results = match_suggestions(query);
        for s in &results {
            assert!(
                (1..=5).contains(&s.cost),
                "Cost {} out of range 1-5 for command '{}' (query: '{}')",
                s.cost,
                s.command,
                query
            );
        }
    }
}

// ── Top suggestion per category ─────────────────────────────────

#[test]
fn test_top_suggestion_per_category() {
    // Verify each category returns the expected highest-relevance tool first
    let expectations = [
        ("sigsegv", "check fold-consistency"),
        ("type mismatch", "type-encoding"),
        ("stack overflow", "check link health"),
        ("infinite loop", "audit-recursion"),
        ("elaboration error", "explain error"),
        ("encoding", "type-encoding"),
        ("mutual recursion", "mutual-recursion-groups"),
    ];
    for (query, expected_top) in &expectations {
        assert_top_suggestion(query, expected_top);
    }
}

// ── Deterministic ordering ──────────────────────────────────────

#[test]
fn test_deterministic_ordering() {
    // Running the same query twice should produce identical results
    let query = "type mismatch in recursive encoding with mutual recursion";
    let run1 = match_suggestions(query);
    let run2 = match_suggestions(query);

    assert_eq!(run1.len(), run2.len(), "Result count differs across runs");
    for (a, b) in run1.iter().zip(run2.iter()) {
        assert_eq!(a.command, b.command, "Command order differs across runs");
        assert_eq!(a.relevance, b.relevance, "Relevance differs across runs");
        assert_eq!(a.cost, b.cost, "Cost differs across runs");
    }
}

// ── Sidecar relevance-key derivation (ADR 23.7.26e D2) ───────────

#[test]
fn test_top_category_maps_verbatim_to_error_class() {
    // Distinct verbatim descriptions in the same class share one category.
    assert_eq!(top_category("sigsegv observed here"), Some("segfault"));
    assert_eq!(
        top_category("segmentation fault at runtime"),
        Some("segfault")
    );
    assert_eq!(
        top_category("type mismatch: expected Nat"),
        Some("type mismatch")
    );
    // Unmatched text has no class.
    assert_eq!(top_category("xyzzy completely unrelated nonsense"), None);
}

#[test]
fn test_command_cost_from_registry() {
    // A known suggestion carries its registry cost tier.
    assert_eq!(command_cost("tungsten diff exec <file>"), Some(5));
    assert_eq!(
        command_cost("tungsten doctor check fold-consistency <file>"),
        Some(3)
    );
    // An unknown command has no known cost.
    assert_eq!(command_cost("not-a-real-command"), None);
}

#[test]
fn test_relevance_context_matched_and_unmatched() {
    // Matched: category is the class, cost from the command.
    let ctx = relevance_context("SIGSEGV crash", "tungsten diff exec <file>");
    assert_eq!(ctx.category, "segfault");
    assert_eq!(ctx.cost, 5);
    // Unmatched: category falls back to the verbatim text; unknown cost is 0.
    let ctx = relevance_context("xyzzy nonsense", "frobnicate");
    assert_eq!(ctx.category, "xyzzy nonsense");
    assert_eq!(ctx.cost, 0);
}

#[test]
fn test_learning_shifts_ranking_for_a_different_verbatim_in_the_same_class() {
    // AC2 (read side): learned relevance for the error *class* — accumulated
    // from OTHER verbatim descriptions — shifts the ranking for a NEW verbatim
    // description in the same class. This is the payoff of the normalized key.
    use crate::sidecar::RelevanceEntry;
    use std::collections::HashMap;

    let cmd = "tungsten info adt <name> <file> --check-fold"; // segfault, base 0.80
    let mut learned = HashMap::new();
    learned.insert(
        cmd.to_string(),
        RelevanceEntry {
            shown_count: 5, // == MIN_SAMPLES
            helped_count: 5,
        },
    );

    let desc = "segmentation fault in a totally different program";
    let baseline = match_suggestions_with_entries(desc, None);
    let boosted = match_suggestions_with_entries(desc, Some(&learned));

    let base_rel = baseline
        .iter()
        .find(|s| s.command == cmd)
        .unwrap()
        .relevance;
    let boosted_rel = boosted.iter().find(|s| s.command == cmd).unwrap().relevance;
    assert!(
        boosted_rel > base_rel,
        "5 helped observations should boost relevance: {boosted_rel} !> {base_rel}"
    );
}

#[test]
fn test_single_observation_below_min_samples_does_not_shift() {
    // AC2: a single report shifts nothing, by design (below MIN_SAMPLES).
    use crate::sidecar::RelevanceEntry;
    use std::collections::HashMap;

    let cmd = "tungsten info adt <name> <file> --check-fold";
    let mut learned = HashMap::new();
    learned.insert(
        cmd.to_string(),
        RelevanceEntry {
            shown_count: 1,
            helped_count: 1,
        },
    );

    let desc = "segfault somewhere";
    let base_rel = match_suggestions_with_entries(desc, None)
        .iter()
        .find(|s| s.command == cmd)
        .unwrap()
        .relevance;
    let with_rel = match_suggestions_with_entries(desc, Some(&learned))
        .iter()
        .find(|s| s.command == cmd)
        .unwrap()
        .relevance;
    assert_eq!(
        base_rel, with_rel,
        "below MIN_SAMPLES must not shift ranking"
    );
}
