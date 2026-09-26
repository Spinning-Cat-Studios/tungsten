//! What the three renderers actually put on the page (ADR 4.9.26d).
//!
//! These assert *text*, which is the point of the seam: before it, every line
//! below was a `println!` inside a `-> ()` function, so the only way to check
//! that a listing was numbered from 1, that an empty answer said anything at
//! all, or that the socket path and the direct path agreed, was to run the
//! binary and read it. The wording is a user-facing surface and drifts like
//! one.

use super::*;

/// A suggestion with the fields a listing renders. Built here rather than
/// imported from a fixture so a field added to `ScoredSuggestion` is a compile
/// error at the one site that renders it.
fn suggestion(command: &'static str, cost: u8, reason: &'static str) -> ScoredSuggestion {
    ScoredSuggestion {
        command,
        cost,
        reason,
        relevance: 0.9,
    }
}

#[test]
fn a_listing_is_numbered_from_one_and_carries_cost_and_reason() {
    let report = human_report(&[
        suggestion("tungsten diff exec <file>", 5, "evaluator vs native"),
        suggestion("tungsten info def <name> <file>", 3, "the Core term"),
    ]);

    assert!(report.starts_with(LISTING_HEADING));
    assert!(report.contains("  1. tungsten diff exec <file>  [cost 5]"));
    assert!(report.contains("     Reason: evaluator vs native"));
    assert!(report.contains("  2. tungsten info def <name> <file>  [cost 3]"));
    // Ranked, so position is meaning: the first suggestion must come first.
    assert!(report.find("  1. ").unwrap() < report.find("  2. ").unwrap());
}

#[test]
fn an_empty_listing_is_the_no_match_answer_and_never_the_heading() {
    let report = human_report(&[]);

    assert_eq!(report, no_match_report());
    assert!(
        !report.contains(LISTING_HEADING),
        "an empty answer must not claim to be ranked: {report}"
    );
    assert!(report.contains("describe what you SAW"));
}

#[test]
fn the_no_match_answer_quotes_every_example_it_offers() {
    let report = no_match_report();
    for example in NO_MATCH_EXAMPLES {
        assert!(
            report.contains(&format!("'{example}'")),
            "the tip omits {example:?}"
        );
    }
}

#[test]
fn the_json_form_round_trips_and_is_an_array_even_when_empty() {
    let json = json_report(&[suggestion("tungsten cache clean", 1, "stale entries")]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["command"], "tungsten cache clean");
    assert_eq!(parsed[0]["cost"], 1);

    let empty: Vec<serde_json::Value> =
        serde_json::from_str(&json_report(&[])).expect("valid JSON");
    assert!(empty.is_empty());
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod socket {
    use super::*;

    #[test]
    fn the_socket_listing_matches_the_direct_one_entry_for_entry() {
        let direct = human_report(&[suggestion("tungsten diff exec <file>", 5, "why")]);
        let socket = socket_report(
            r#"[{"command": "tungsten diff exec <file>", "cost": 5, "reason": "why"}]"#,
        );
        assert_eq!(
            socket, direct,
            "the two render paths must not drift — they are one answer"
        );
    }

    #[test]
    fn an_empty_socket_answer_is_the_same_no_match_answer() {
        assert_eq!(socket_report("[]"), no_match_report());
    }

    #[test]
    fn an_unparseable_socket_answer_is_echoed_rather_than_swallowed() {
        let report = socket_report("{not json at all");
        assert!(
            report.contains("{not json at all"),
            "the sidecar's answer must survive to be reported: {report}"
        );
    }

    #[test]
    fn a_missing_field_renders_a_placeholder_rather_than_dropping_the_entry() {
        let report = socket_report(r#"[{"command": "tungsten cache clean"}]"#);
        assert!(report.contains("  1. tungsten cache clean  [cost 0]"));
        assert!(report.contains("     Reason: ?"));
    }
}
