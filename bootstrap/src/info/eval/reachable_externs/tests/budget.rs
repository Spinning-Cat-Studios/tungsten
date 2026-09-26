//! The walk budget and its third terminal state (ADR 3.9.26c D2 / AC3 / AC4).
//!
//! The case each of these guards is one shape: a partial walk that reads like a
//! complete clean one. `blocking: 0` over a graph the walk never entered is the
//! believable wrong answer, and it is believable in exactly the direction that
//! sends a reader off to write an assertion that will never run.

use std::path::PathBuf;

use super::super::render::{render_human, render_json};
use super::*;

/// A term referencing every named global, so one definition can have an
/// arbitrary out-degree.
fn calls_all(names: &[&str]) -> Term {
    names.iter().skip(1).fold(calls(names[0]), |acc, name| {
        Term::Pair(Box::new(acc), Box::new(calls(name)))
    })
}

/// `root → a → b → blocked_leaf`, where only the leaf reaches an unexecutable
/// extern. Any budget below 4 hides the finding, which is the point.
fn chain_to_a_blocking_leaf() -> BTreeMap<String, Term> {
    project(&[
        ("root", calls("a")),
        ("a", calls("b")),
        ("b", calls("blocked_leaf")),
        ("blocked_leaf", extern_wrapper("tg_path_join")),
    ])
}

#[test]
fn a_walk_inside_its_budget_reports_complete_and_finds_everything() {
    let report = analyze_bounded(&chain_to_a_blocking_leaf(), "root", 4).unwrap();

    assert!(report.complete());
    assert!(report.not_reached.is_empty());
    assert_eq!(report.defs_visited, 4);
    assert_eq!(report.blocking().count(), 1);
}

#[test]
fn a_budget_below_the_graph_stops_the_walk_and_names_what_it_did_not_enter() {
    let report = analyze_bounded(&chain_to_a_blocking_leaf(), "root", 2).unwrap();

    assert!(!report.complete());
    assert_eq!(report.defs_visited, 2);
    assert_eq!(report.not_reached, vec!["b".to_string()]);
    // The whole hazard, stated: the finding is now invisible.
    assert_eq!(report.blocking().count(), 0);
}

#[test]
fn a_partial_walk_never_claims_the_call_path_is_clean() {
    let report = analyze_bounded(&chain_to_a_blocking_leaf(), "root", 2).unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert!(text.contains("⚠ INCOMPLETE"), "{text}");
    assert!(text.contains("PARTIAL answer"), "{text}");
    assert!(text.contains("not reached: b"), "{text}");
    assert!(text.contains("--max-visited"), "{text}");
    assert!(!text.contains("✓ Every extern"), "{text}");
    // A list short enough to print whole carries no elision clause at all —
    // `(+0 more)` beside a complete list would be its own small lie.
    assert!(!text.contains("(+"), "{text}");
}

#[test]
fn a_partial_walk_is_never_an_untested_opportunity() {
    // `assertable_but_untested` is a "nothing blocks" claim. Over a budgeted
    // walk it would be an assertion about definitions nobody read.
    let report = analyze_bounded(&chain_to_a_blocking_leaf(), "root", 2).unwrap();

    assert!(report.reached_by_tests.is_empty());
    assert!(!report.assertable_but_untested());
    assert!(
        !render_human(&report, &PathBuf::from("m.tg")).contains("ASSERTABLE, AND NOTHING"),
        "a partial walk must not advertise unused coverage"
    );
}

#[test]
fn the_json_carries_an_explicit_complete_field_beside_a_zero_blocking_count() {
    // AC3's gate case: a consumer branching on `blocking` must be able to see,
    // in one key, that the zero came from a walk that stopped early.
    let report = analyze_bounded(&chain_to_a_blocking_leaf(), "root", 2).unwrap();
    let json = render_json(&report);

    assert!(json.contains("\"blocking\": 0"), "{json}");
    assert!(json.contains("\"complete\": false"), "{json}");
    assert!(json.contains("\"not_reached\": [\"b\"]"), "{json}");
    assert!(
        json.contains("\"assertable_but_untested\": false"),
        "{json}"
    );
}

#[test]
fn a_blocking_finding_survives_an_incomplete_walk() {
    // An extern that WAS reached stays reached, so the ✗ verdict is still
    // sound — the budget weakens the clean verdict, not the positive one.
    let globals = project(&[
        (
            "root",
            Term::Pair(
                Box::new(extern_wrapper("tg_path_join")),
                Box::new(calls("deeper")),
            ),
        ),
        ("deeper", Term::Unit),
    ]);
    let report = analyze_bounded(&globals, "root", 1).unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert_eq!(report.blocking().count(), 1);
    assert_eq!(report.not_reached, vec!["deeper".to_string()]);
    assert!(text.contains("silently Stuck"), "{text}");
    assert!(text.contains("⚠ INCOMPLETE"), "{text}");
}

#[test]
fn a_zero_budget_examines_nothing_and_says_which_it_was() {
    let globals = project(&[("root", extern_wrapper("tg_path_join"))]);
    let report = analyze_bounded(&globals, "root", 0).unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    // Not even the root: `0 definition(s) walked` must read differently from
    // `1 definition(s) walked, 0 extern(s) reached`.
    assert_eq!(report.defs_visited, 0);
    assert_eq!(report.not_reached, vec!["root".to_string()]);
    assert!(!report.complete());
    assert!(text.contains("0 definition(s) walked"), "{text}");
    assert!(text.contains("⚠ INCOMPLETE"), "{text}");
}

#[test]
fn a_zero_budget_still_reports_failure_for_a_definition_that_does_not_exist() {
    // The budget must not turn "no such definition" into a partial answer about
    // one — those are the two errors a reader most needs kept apart.
    let globals = project(&[("root", Term::Unit)]);
    assert!(analyze_bounded(&globals, "absent", 0).is_none());
}

#[test]
fn the_human_report_caps_the_not_reached_list_and_counts_the_remainder() {
    let names: Vec<String> = (1..=12).map(|n| format!("d{n:02}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut defs: Vec<(&str, Term)> = vec![("root", calls_all(&refs))];
    defs.extend(refs.iter().map(|n| (*n, Term::Unit)));

    let report = analyze_bounded(&project(&defs), "root", 1).unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert_eq!(report.not_reached.len(), 12);
    assert!(text.contains("d10"), "{text}");
    assert!(!text.contains("d11"), "{text}");
    assert!(text.contains("(+2 more; --json lists them all)"), "{text}");
    // The full list is never lost, only moved.
    assert!(render_json(&report).contains("\"d11\""));
}

#[test]
fn the_default_budget_cannot_bound_a_walk_over_a_real_entry_file() {
    // AC4: a walk visits each definition at most once, and `src/compiler/main.tg`
    // holds ~2300. A default above that cannot change an answer that completes
    // today — which is why this is an inequality, not a golden value.
    assert!(
        DEFAULT_MAX_VISITED >= 10_000,
        "default budget {DEFAULT_MAX_VISITED} must stay far above any entry file's definition count"
    );

    let report = analyze_default(&chain_to_a_blocking_leaf(), "root").unwrap();
    assert!(report.complete());
    assert_eq!(report.blocking().count(), 1);
}
