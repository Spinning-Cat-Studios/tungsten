//! Keyword-table hygiene for `tungsten doctor suggest-tools` (ADR 24.7.26e).
//!
//! These guard the MATCHER CONTRACT rather than any single suggestion: that a
//! keyword can physically match, and that the retired level vocabulary the
//! rename kept as legacy synonyms still routes to the renamed tool.

use super::*;

/// `scored_patterns` lowercases the user's description and then tests
/// `desc_lower.contains(keyword)`. A keyword carrying an uppercase letter can
/// therefore NEVER match — it is dead weight that reads like a working
/// synonym. ADR 24.7.26e found 12 such keywords, six of them the level
/// synonyms the ADR had just decided to preserve for exactly this purpose.
#[test]
fn keywords_are_lowercase() {
    let offenders: Vec<(&str, &str)> = all_patterns()
        .flat_map(|p| p.keywords.iter().map(move |kw| (p.category, *kw)))
        .filter(|(_, kw)| kw.chars().any(|c| c.is_uppercase()))
        .collect();
    assert!(
        offenders.is_empty(),
        "keywords must be lowercase or they can never match: {offenders:?}"
    );
}

/// The ADR 24.7.26e legacy synonyms must actually resolve — the whole point of
/// retaining them is that a user typing the retired vocabulary still gets a
/// hit. Each query below is routed by a level synonym alone.
#[test]
fn retired_level_vocabulary_still_finds_the_divergence_tool() {
    for query in [
        "L2 fails but L1 passes",
        "l2 regression",
        "L3 disagrees",
        "l1-l2 mismatch",
    ] {
        let results = match_suggestions(query);
        assert!(
            results
                .iter()
                .any(|s| s.command.contains("bootstrap-selfhost-check")),
            "legacy synonym query {query:?} found nothing"
        );
    }
}

/// The inspection tier exists because of a measured miss: every phrasing of
/// "show me the elaborated term" returned nothing, so the session that needed
/// it wrote a throwaway printing test instead. These are the exact queries
/// that failed, pinned so the route cannot silently disappear again.
#[test]
fn show_me_the_core_term_questions_find_info_def() {
    for query in [
        "core term",
        "core ir",
        "what does an elaborated function look like in core ir",
        "inspect a definition",
        "dump the term for a function",
        "what did this match lower to",
    ] {
        let results = match_suggestions(query);
        assert!(
            results.iter().any(|s| s.command.contains("info def")),
            "inspection query {query:?} did not route to `tungsten info def`"
        );
    }
}

/// Inspection must rank LAST: a description that mentions both a crash and a
/// term dump has to keep the crash tools on top, which is the same ordering
/// rule `profiling` was given (ADR 5.8.26b).
#[test]
fn a_crash_outranks_an_inspection_request_in_the_same_description() {
    let results = match_suggestions("segfault while dumping the core term");
    let first = results.first().expect("a segfault query matches something");

    assert!(
        !first.command.contains("info def"),
        "inspection outranked the crash tools: {results:?}"
    );
}

/// A wedged *compiler* and a looping *program* share the word "hang" and share
/// none of their tools. The wedge route must lead with `sample`, because every
/// elaborating diagnostic hangs on the input that wedged the compiler.
#[test]
fn a_wedged_compiler_is_told_to_sample_before_anything_elaborating() {
    for query in [
        "check hangs during body elaboration",
        "compiler hangs with no output",
        "tungsten is wedged at constant rss",
    ] {
        let results = match_suggestions(query);
        let first = results
            .first()
            .unwrap_or_else(|| panic!("wedge query {query:?} found nothing"));
        assert!(
            first.command.contains("sample"),
            "wedge query {query:?} led with {:?}, not `sample`",
            first.command
        );
    }
}

/// The program-looping route is unchanged by the wedge split — `audit-recursion`
/// still leads for a description about the compiled program.
#[test]
fn a_looping_program_still_leads_with_audit_recursion() {
    let results = match_suggestions("my program has an infinite loop");
    let first = results.first().expect("a loop query matches something");

    assert!(
        first.command.contains("audit-recursion"),
        "program-loop query led with {:?}",
        first.command
    );
}

/// A pattern with no suggestions is a table entry that can only waste a
/// reader's query. Asserted separately from the reachability walk above so the
/// two failures do not read as one.
#[test]
fn every_pattern_offers_at_least_one_suggestion() {
    let empty: Vec<&str> = all_patterns()
        .filter(|p| p.suggestions.is_empty())
        .map(|p| p.category)
        .collect();
    assert!(empty.is_empty(), "categories suggesting nothing: {empty:?}");
}

/// Categories with a per-category test naming their tool, kept in step with
/// the registry by [`every_registry_category_has_a_per_category_test`].
///
/// A list rather than a derived set, deliberately: the thing being reconciled
/// is "someone wrote a test", and only a human can add that. Adding a name here
/// without writing the test defeats it — but so does any coverage convention,
/// and this one at least fails loudly at the moment of omission.
const COVERED_CATEGORIES: &[&str] = &[
    "abi mismatch",
    "bootstrap/self-host divergence",
    "change-scoped quality gate",
    "cir variant lookup",
    "colliding-name misresolution",
    "comparator-stuck",
    "compile-time hotspot",
    "compiler wedge",
    "constructor / duplicate registration",
    "cross-file error",
    "elaboration error",
    "encoding / μ-type",
    "encoding nondeterminism",
    "import alias",
    "infinite loop",
    "inspect a definition",
    "inspect an encoding",
    "linker / self-compile",
    "match dispatch",
    "merge-arms lowering divergence",
    "miscompile / wrong value",
    "mutual recursion",
    "nested pattern",
    "private-item access / name collision",
    "profile symbol attribution",
    "record field",
    "referenced but not declared",
    "segfault",
    "stack overflow",
    "stale/warm elaboration cache",
    "termination",
    "termination-proof-boundary",
    "type mismatch",
    "unexplained proof hole",
];

/// **A new pattern must arrive with a test.**
///
/// The registry is `const` data: mutation testing scores it at zero, and a
/// per-category test is opt-in, so a pattern added without one is guarded by
/// nothing whatsoever — and it fails *silently*, because the tool still runs,
/// still ranks, and simply never mentions the command someone added it for.
/// ADR 13.8.26c's review found exactly that hole, one pattern old.
///
/// Reconciling the two lists is what makes the omission loud. It deliberately
/// does NOT try to prove reachability from the table itself: querying a
/// pattern's own keyword matches that pattern by construction, and the matcher
/// neither truncates nor thresholds, so such a test passes for every possible
/// table and proves nothing.
///
/// Fails in **both** directions — a stale name here outlives the pattern it
/// described and would otherwise sit forever, claiming coverage of nothing.
#[test]
fn every_registry_category_has_a_per_category_test() {
    let registry: std::collections::BTreeSet<&str> = all_patterns().map(|p| p.category).collect();
    let covered: std::collections::BTreeSet<&str> = COVERED_CATEGORIES.iter().copied().collect();

    let untested: Vec<&&str> = registry.difference(&covered).collect();
    assert!(
        untested.is_empty(),
        "pattern(s) with no per-category test — add one in `tests.rs`, then list \
         the category here: {untested:?}"
    );

    let stale: Vec<&&str> = covered.difference(&registry).collect();
    assert!(
        stale.is_empty(),
        "listed categor(ies) no longer in the registry — delete the name and its \
         test: {stale:?}"
    );
}
