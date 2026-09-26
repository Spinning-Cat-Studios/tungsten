//! Per-category tests: does a query a user would actually paste surface the
//! command for that category?
//!
//! One block per `ErrorPattern` category, in registry order. The invariants
//! that hold across the WHOLE table — empty input, case folding, the relevance
//! cap, cost-field validity, deterministic ordering — are next door in
//! [`super::matcher_property_tests`]; they were one file until this one reached
//! its size ceiling, and the seam is real: a test here fails when one category's
//! keywords stop matching, a test there fails when the matcher itself does.
//!
//! The shared `assert_suggests` / `assert_top_suggestion` harness lives here
//! because this is where it is used most; both are `pub(super)`.

use super::output::{no_match_report, NO_MATCH_EXAMPLES};
use super::*;

/// Helper: assert that the suggestions for a query contain a specific command substring.
///
/// `pub(super)` so sibling test modules share one harness rather than copying
/// it — `termination_tests` is the first such sibling.
pub(super) fn assert_suggests(query: &str, expected_cmd: &str) {
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
pub(super) fn assert_top_suggestion(query: &str, expected_cmd: &str) {
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

// ── Category: stale/warm elaboration cache (ADR 4.7.26c) ─────────

#[test]
fn test_warm_cache_symptoms_suggest_cache_clean() {
    // The symptom cluster that had no pattern before ADR 4.7.26c — each of
    // these should now route to `cache clean` as the top suggestion.
    for query in [
        "tungsten run reports no main function found",
        "test prints no tests found on the second run",
        "span out of bounds this may indicate a file_path tracking issue",
        "warm cache gives empty defs",
    ] {
        assert_top_suggestion(query, "cache clean");
    }
    // ADR 4.7.26d also adds `cache inspect` + `diff cache` to this cluster; that
    // listing is asserted end-to-end in tests/cache_diagnostics.rs (real binary).
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
    assert_suggests(
        "unresolved type reference",
        "check type integrity phase-invariants",
    );
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

// ── Category: miscompile / wrong value (ADR 3.7.26d) ────────────

/// The ADR 3.7.26a cold start, now a cost-1 answer: a wrong-runtime-value
/// query returns a non-empty ranked list topped by `diff exec`.
#[test]
fn test_wrong_value_top_is_diff_exec() {
    assert_top_suggestion("compiled binary returns wrong value", "diff exec");
}

#[test]
fn test_miscompile_keywords_reach_the_pattern() {
    // Canonical keyword set (ADR 3.7.26d §2.3) — every keyword matches.
    for query in [
        "wrong value at runtime",
        "binary prints garbage",
        "suspected miscompile",
        "evaluator differs from native output",
        "incorrect output from compiled program",
        "prints wrong number",
    ] {
        assert_top_suggestion(query, "diff exec");
    }
}

#[test]
fn test_miscompile_suggests_sret_stores() {
    assert_suggests(
        "compiled binary returns wrong value",
        "check ir sret-stores",
    );
}

#[test]
fn test_miscompile_suggests_ir_inspection() {
    assert_suggests("suspected miscompile", "--dump-ir");
    assert_suggests("suspected miscompile", "--emit-llvm");
}

// ── Category: merge-arms lowering divergence (ADR 12.7.26c) ─────

/// Each phrase from the merge-arms T2 error routes to the lowering-consistency
/// check — the error whose own wording scored zero before this ADR.
#[test]
fn test_merge_divergence_phrases_suggest_lowering_consistency() {
    for query in [
        "merge arms disagree on result type",
        "two reachable arms lowered to different types",
        "reachable arms lowered to different types",
    ] {
        assert_top_suggestion(query, "check type lowering-consistency");
    }
}

#[test]
fn test_merge_divergence_suggests_info_type_lowering_and_diff() {
    assert_suggests("merge arms disagree", "info type lowering");
    assert_suggests("lowered to different types", "diff ir");
    assert_suggests("merge arms disagree", "diff exec");
}

// ── Category: referenced but not declared (ADR 1.7.26f vocabulary) ─

#[test]
fn test_referenced_but_not_declared_suggests_codegen_symbols() {
    assert_top_suggestion(
        "symbol referenced but not declared in depot",
        "info codegen symbols",
    );
    assert_suggests("referenced but not declared", "check link collisions");
}

// ── Category: bootstrap/self-host divergence (ADR 4.9.26d) ──────

/// The commands `'self-host divergence'` returns, in the order it returns them.
/// Written out rather than derived so that a widened trigger set cannot quietly
/// re-rank the answer the cause-shaped query already gave (AC 4).
const DIVERGENCE_RANKING: &[&str] = &[
    "tungsten diff bootstrap-selfhost-check <file> --selfhost-binary ./tungsten1",
    "tungsten diff selfhost-core <def> <file>",
    "tungsten doctor check selfhost closed-terms <file>",
    "tungsten doctor check selfhost well-typed-terms <file>",
    "tungsten doctor check nested-patterns <file>",
    "tungsten explain error --self-hosted <code>",
];

/// **The measured misses now match** (ADR 4.9.26d AC 2). These three queries
/// returned *no matching diagnostic tools* on 4 Sep 2026; each is the symptom a
/// reader of the 3.9.26g projection bug actually had, none of them names the
/// divergence, and each must now reach the tools that answer it.
#[test]
fn the_three_measured_misses_reach_the_divergence_tools() {
    for query in [
        "wrong field value",
        "destructuring gives the wrong element",
        "a projection builder disagrees with the type walk beside it",
    ] {
        let results = match_suggestions(query);
        assert!(
            !results.is_empty(),
            "measured-miss query {query:?} still returns nothing"
        );
        for expected in DIVERGENCE_RANKING {
            assert!(
                results.iter().any(|s| s.command == *expected),
                "{query:?} did not offer {expected:?}; got {:?}",
                results.iter().map(|s| s.command).collect::<Vec<_>>()
            );
        }
    }
}

/// **The cause-shaped queries are untouched** (AC 4). A wider trigger set earns
/// nothing if it dilutes the ranking for the reader who already knew what to
/// type: both phrasings still return exactly these six commands, in this order.
#[test]
fn the_cause_shaped_queries_keep_their_exact_ranking() {
    for query in ["self-host divergence", "bootstrap and self-host disagree"] {
        let commands: Vec<&str> = match_suggestions(query).iter().map(|s| s.command).collect();
        assert_eq!(
            commands, DIVERGENCE_RANKING,
            "the ranking for {query:?} changed"
        );
    }
}

// ── The empty answer (ADR 4.9.26d D4/AC 6) ──────────────────────

/// A query that genuinely matches nothing still says so, and says it in the
/// reader's own vocabulary. The report is not empty, it names the failure, and
/// it is plainly distinguishable from the "most relevant first" listing a match
/// produces.
#[test]
fn a_query_that_matches_nothing_still_answers() {
    let results = match_suggestions("xyzzy completely unrelated nonsense");
    assert!(results.is_empty(), "expected a genuine no-match query");

    let report = no_match_report();
    assert!(report.contains("No matching diagnostic tools found"));
    assert!(
        !report.contains("most relevant first"),
        "the empty answer must not read like a listing"
    );
}

/// **The tip must not lie.** Every example the empty answer offers is itself a
/// query that matches — the failure this replaces was a tip whose every example
/// was cause-shaped, i.e. the one lesson the surface must not teach.
#[test]
fn every_example_in_the_no_match_tip_actually_matches() {
    for example in NO_MATCH_EXAMPLES {
        assert!(
            !match_suggestions(example).is_empty(),
            "the no-match tip offers {example:?}, which itself returns nothing"
        );
        assert!(
            no_match_report().contains(&format!("'{example}'")),
            "the no-match tip must quote {example:?} verbatim"
        );
    }
}

// ── Category: private-item access / name collision (ADR 13.8.26c) ─

/// The message a reader actually pastes in. E0016 is reported in the LOSER's
/// file naming the WINNER's module, so its own text points away from the edit
/// that caused it — which makes `suggest-tools` the surface that has to name
/// the tool, since nothing in the error does.
#[test]
fn a_private_item_error_tops_out_at_name_collisions() {
    assert_top_suggestion(
        "function `string_slice` is private (defined in `codegen::ir_header`) \
         and cannot be accessed from `elab::error::source`",
        "check module name-collisions",
    );
}

/// The code and the bare phrasing route there too — a reader may paste either.
#[test]
fn the_code_and_the_bare_phrasing_both_reach_the_check() {
    assert_suggests("E0016", "check module name-collisions");
    assert_suggests("is private", "check module name-collisions");
    assert_suggests("name collision", "check module name-collisions");
}

/// The check is **advisory** (ADR 13.8.26c D3), so `suggest-tools` is doing the
/// work an exit code is not: the two cheaper companions must come with it,
/// because the recommendation is the whole discovery path for this tool.
#[test]
fn the_cheap_companions_ride_along_with_it() {
    assert_suggests("cannot be accessed from", "explain error E0016");
    assert_suggests("cannot be accessed from", "info error-sites E0016");
}

// ── Category: unexplained proof hole (ADR 18.9.26g) ─────────────

// 18.9.26g AC3: the symptom ranks the census first.
#[test]
fn test_unwritten_sorry_top_is_sorry_sites() {
    assert_top_suggestion(
        "check says contains sorry but I wrote no sorry",
        "doctor check sorry-sites",
    );
    assert_top_suggestion("contains sorry", "doctor check sorry-sites");
}
