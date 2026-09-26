//! The symptom-query corpus and its reachability gate (ADR 4.9.26d).
//!
//! `suggest-tools` is the mandated first step of every compiler-error
//! investigation, and CLAUDE.md tells its reader to type the **symptom**. For
//! several patterns that instruction did not work: the queries that matched all
//! contained the diagnosis already, so the reader who knew what to type did not
//! need the tool and the reader who did not got *no matching diagnostic tools*.
//!
//! This corpus is the measurement that decided the fix and the gate that keeps
//! it true. Every registry category carries at least one query phrased the way
//! someone who has **not** yet diagnosed the problem would phrase it — what they
//! saw, not what caused it — and every one of those must reach its pattern.
//!
//! Baseline, measured against the table as it stood before any keyword was
//! added: **25 of 67 queries reached their pattern (37%)**, and **14 of the 33
//! categories were reachable by no symptom phrasing at all** — segfault and
//! `infinite loop` among the survivors only because "segmentation fault" and
//! "never returns" are things a user literally sees. That is the number D2 asked
//! for and the refutation point it named: the defect was the matcher's
//! vocabulary, not one pattern's, so the ADR kept its full size.
//!
//! The sibling gate in [`super::keyword_hygiene_tests`] reconciles categories
//! against *hand-written per-category tests*; this one reconciles them against
//! *symptom phrasings*. They fail for different reasons and are deliberately
//! separate: a category can have a test naming its tool and still be reachable
//! only by someone who already knows the answer.

use super::*;

mod queries;

pub(super) use queries::SYMPTOM_QUERIES;

/// Does `query` reach the pattern registered under `category`?
///
/// Reachability, not ranking: the pattern is among the ones the matcher scored,
/// so every one of its suggestions is in the output (deduplicated against
/// higher-scoring patterns, which keeps the *command* either way). Ranking is
/// asserted per category next door in [`super::tests`], where the expected top
/// command is written out.
fn reaches(query: &str, category: &str) -> bool {
    scored_patterns(&query.to_lowercase())
        .iter()
        .any(|(_, p)| p.category == category)
}

/// **`reaches` must be able to say no.** The corpus gate below is a wall of
/// `assert!`s over one predicate, so a predicate that answered `true`
/// unconditionally would pass it with an empty table and a wrong registry —
/// the vacuity that makes a green suite worthless. These two pairs pin both
/// answers on the same query: it reaches the class it describes and not a
/// neighbouring one.
#[test]
fn reaches_distinguishes_the_class_a_query_describes_from_one_it_does_not() {
    assert!(reaches("my recursive function is rejected", "termination"));
    assert!(!reaches(
        "my recursive function is rejected",
        "record field"
    ));

    assert!(reaches("wrong field value", "record field"));
    assert!(!reaches("wrong field value", "compile-time hotspot"));
}

/// **The corpus must stay true.** Each symptom phrasing reaches the pattern it
/// was written for; the failure message is the miss list, which is also how the
/// baseline in this module's header was measured.
#[test]
fn every_symptom_query_reaches_its_pattern() {
    let misses: Vec<String> = SYMPTOM_QUERIES
        .iter()
        .flat_map(|(category, queries)| {
            queries
                .iter()
                .filter(move |q| !reaches(q, category))
                .map(move |q| format!("{category}: {q:?}"))
        })
        .collect();

    let total: usize = SYMPTOM_QUERIES.iter().map(|(_, qs)| qs.len()).sum();
    assert!(
        misses.is_empty(),
        "{}/{total} symptom queries did not reach their pattern:\n  {}",
        misses.len(),
        misses.join("\n  ")
    );
}

/// **A pattern added without a symptom trigger fails the build** (ADR 4.9.26d
/// D3). Fails in both directions, like its sibling in `keyword_hygiene_tests`:
/// a corpus entry for a category the registry no longer has is a stale claim of
/// coverage.
#[test]
fn every_registry_category_has_a_symptom_query() {
    let registry: std::collections::BTreeSet<&str> = all_patterns().map(|p| p.category).collect();
    let corpus: std::collections::BTreeSet<&str> = SYMPTOM_QUERIES
        .iter()
        .map(|(category, _)| *category)
        .collect();

    let uncovered: Vec<&&str> = registry.difference(&corpus).collect();
    assert!(
        uncovered.is_empty(),
        "pattern(s) reachable only by someone who already knows the answer — add \
         a symptom-shaped query for: {uncovered:?}"
    );

    let stale: Vec<&&str> = corpus.difference(&registry).collect();
    assert!(
        stale.is_empty(),
        "corpus entr(ies) for categor(ies) no longer in the registry: {stale:?}"
    );

    for (category, queries) in SYMPTOM_QUERIES {
        assert!(
            !queries.is_empty(),
            "category {category:?} has an empty query list"
        );
    }
}

/// **A query that restates the category name is not a symptom.** The floor this
/// enforces is deliberately mechanical and deliberately low — it catches the
/// one cheat that would hollow the corpus out (pasting the registry's own
/// vocabulary back in) and cannot judge phrasing beyond that. Multi-part
/// category names are checked part by part, so `miscompile / wrong value`
/// forbids both halves.
#[test]
fn symptom_queries_do_not_name_their_own_category() {
    let offenders: Vec<String> = SYMPTOM_QUERIES
        .iter()
        .flat_map(|(category, queries)| {
            category.split('/').map(str::trim).flat_map(move |part| {
                queries
                    .iter()
                    .filter(move |q| q.contains(part))
                    .map(move |q| format!("{category}: {q:?} restates {part:?}"))
            })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "symptom queries must describe what was OBSERVED, not name the class:\n  {}",
        offenders.join("\n  ")
    );
}

/// The queries are matched with `contains` against a lowercased description, so
/// an uppercase letter in the corpus would test a path the matcher never takes.
#[test]
fn symptom_queries_are_lowercase() {
    let offenders: Vec<&&str> = SYMPTOM_QUERIES
        .iter()
        .flat_map(|(_, queries)| queries.iter())
        .filter(|q| q.chars().any(char::is_uppercase))
        .collect();
    assert!(
        offenders.is_empty(),
        "corpus queries must be lowercase to exercise the real matcher path: {offenders:?}"
    );
}
