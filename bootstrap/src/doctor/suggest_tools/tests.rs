use super::*;

/// Helper: assert that the suggestions for a query contain a specific command substring.
fn assert_suggests(query: &str, expected_cmd: &str) {
    let results = match_suggestions(query);
    assert!(
        results.iter().any(|s| s.command.contains(expected_cmd)),
        "Expected '{}' in suggestions for '{}', got: {:?}",
        expected_cmd,
        query,
        results.iter().map(|s| s.command).collect::<Vec<_>>()
    );
}

/// Helper: assert that the first suggestion for a query contains a specific command.
fn assert_top_suggestion(query: &str, expected_cmd: &str) {
    let results = match_suggestions(query);
    assert!(
        !results.is_empty(),
        "Expected suggestions for '{}', got none",
        query
    );
    assert!(
        results[0].command.contains(expected_cmd),
        "Expected top suggestion to contain '{}' for '{}', got '{}'",
        expected_cmd,
        query,
        results[0].command
    );
}

// ── Category: segfault ──────────────────────────────────────────

#[test]
fn test_sigsegv_suggests_fold_consistency() {
    assert_suggests(
        "SIGSEGV when running compiled program",
        "check fold-consistency",
    );
}

#[test]
fn test_segfault_suggests_ir_layout() {
    assert_suggests("segfault in compiled output", "check ir-layout");
}

#[test]
fn test_sigsegv_top_is_fold_consistency() {
    assert_top_suggestion("SIGSEGV crash", "check fold-consistency");
}

// ── Category: type mismatch ─────────────────────────────────────

#[test]
fn test_type_mismatch_suggests_type_encoding() {
    assert_suggests("type mismatch: expected Nat, got Bool", "type-encoding");
}

#[test]
fn test_type_mismatch_suggests_diff_types() {
    assert_suggests("type error in function return", "diff types");
}

#[test]
fn test_type_mismatch_suggests_trace_types() {
    assert_suggests("type mismatch in elaboration", "trace-types");
}

// ── Category: stack overflow ────────────────────────────────────

#[test]
fn test_stack_overflow_suggests_explain() {
    assert_suggests(
        "stack overflow in recursive function",
        "explain stack-overflow",
    );
}

#[test]
fn test_stack_overflow_suggests_audit_recursion() {
    assert_suggests("stack overflow crash", "audit-recursion");
}

#[test]
fn test_stack_overflow_suggests_encoding_depth() {
    assert_suggests("stack overflow", "check encoding-depth");
}

// ── Category: infinite loop ─────────────────────────────────────

#[test]
fn test_hang_suggests_audit_recursion() {
    assert_suggests("program hang, not terminating", "audit-recursion");
}

#[test]
fn test_infinite_loop_suggests_recursion_types() {
    assert_suggests("infinite loop detected", "recursion-types");
}

// ── Category: elaboration error ─────────────────────────────────

#[test]
fn test_elaboration_error_suggests_explain() {
    assert_suggests("elaboration error: unknown constructor", "explain error");
}

#[test]
fn test_undefined_suggests_phase_invariants() {
    assert_suggests("unresolved type reference", "check phase-invariants");
}

// ── Category: encoding / μ-type ─────────────────────────────────

#[test]
fn test_encoding_suggests_type_encoding() {
    assert_suggests("wrong encoding for recursive type", "type-encoding");
}

#[test]
fn test_mu_type_suggests_trace_encoding() {
    assert_suggests("μ-type encoding looks wrong", "trace-encoding");
}

#[test]
fn test_tyvar_suggests_type_encoding() {
    assert_suggests("TyVar escape in encoding", "type-encoding");
}

// ── Category: mutual recursion ──────────────────────────────────

#[test]
fn test_mutual_recursion_suggests_groups() {
    assert_suggests("mutual recursion between types", "mutual-recursion-groups");
}

#[test]
fn test_cycle_suggests_audit_mutual() {
    assert_suggests("circular type dependency detected", "audit-mutual-types");
}

#[test]
fn test_mutual_recursion_suggests_fold() {
    assert_suggests("mutual recursion fold/unfold", "check fold-consistency");
}

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
        ("stack overflow", "check link-health"),
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
