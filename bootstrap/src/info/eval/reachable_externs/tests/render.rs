//! The two output modes for one root — the single-definition spelling that
//! every existing ADR's evidence was taken with (ADR 3.9.26c D3/AC5).

use std::path::PathBuf;

use super::super::render::{render_human, render_json};
use super::*;

#[test]
fn the_human_report_names_the_counts_when_nothing_was_reached() {
    // The 11.8.26c lesson: `0 examined` and `0 violations` must never render
    // alike. A reader must be able to tell "walked one definition, it calls
    // nothing" from "walked nothing at all".
    let globals = project(&[("main", Term::Unit)]);
    let report = analyze_default(&globals, "main").unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert!(text.contains("1 definition(s) walked"), "{text}");
    assert!(text.contains("0 extern(s) reached"), "{text}");
    assert!(text.contains("makes no extern call"), "{text}");
    assert!(
        text.contains("✓ Every extern on this call path is executable"),
        "{text}"
    );
    assert!(!text.contains("INCOMPLETE"), "{text}");
}

#[test]
fn the_human_report_shows_the_chain_and_the_stuck_warning_when_blocking() {
    let globals = project(&[
        ("harness_path", calls("path_join")),
        ("path_join", extern_wrapper("tg_path_join")),
    ]);
    let report = analyze_default(&globals, "harness_path").unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert!(text.contains("harness_path → path_join"), "{text}");
    assert!(text.contains("1 NOT executable"), "{text}");
    assert!(text.contains("silently Stuck"), "{text}");
    assert!(!text.contains("✓ Every extern"), "{text}");
}

#[test]
fn the_human_report_names_unresolved_globals() {
    let globals = project(&[("main", calls("nowhere"))]);
    let report = analyze_default(&globals, "main").unwrap();
    let text = render_human(&report, &PathBuf::from("m.tg"));

    assert!(text.contains("were not followed"), "{text}");
    assert!(text.contains("nowhere"), "{text}");
}

#[test]
fn the_json_report_carries_the_blocking_count_and_the_chain() {
    let globals = project(&[
        ("harness_path", calls("path_join")),
        ("path_join", extern_wrapper("tg_path_join")),
    ]);
    let report = analyze_default(&globals, "harness_path").unwrap();
    let json = render_json(&report);

    assert!(json.contains("\"blocking\": 1"), "{json}");
    assert!(json.contains("\"defs_visited\": 2"), "{json}");
    assert!(json.contains("\"symbol\": \"tg_path_join\""), "{json}");
    assert!(json.contains("\"executable\": false"), "{json}");
    assert!(
        json.contains("\"via\": [\"harness_path\", \"path_join\"]"),
        "{json}"
    );
}

#[test]
fn the_json_report_is_clean_when_nothing_blocks() {
    let globals = project(&[("shout", extern_wrapper("tg_println"))]);
    let report = analyze_default(&globals, "shout").unwrap();
    let json = render_json(&report);

    assert!(json.contains("\"blocking\": 0"), "{json}");
    assert!(json.contains("\"executable\": true"), "{json}");
    assert!(json.contains("\"unresolved\": []"), "{json}");
    assert!(json.contains("\"complete\": true"), "{json}");
    assert!(json.contains("\"not_reached\": []"), "{json}");
}

#[test]
fn the_json_report_carries_the_untested_flag() {
    let globals = project(&[("shout", extern_wrapper("tg_println"))]);
    let report = analyze_default(&globals, "shout").unwrap();
    let json = render_json(&report);

    assert!(json.contains("\"assertable_but_untested\": true"), "{json}");
    assert!(json.contains("\"reached_by_tests\": []"), "{json}");
}

#[test]
fn the_human_report_names_the_opportunity_and_stays_quiet_otherwise() {
    let untested = project(&[("shout", extern_wrapper("tg_println"))]);
    let report = analyze_default(&untested, "shout").unwrap();
    let text = render_human(&report, &PathBuf::from("f.tg"));
    assert!(
        text.contains("ASSERTABLE, AND NOTHING ASSERTS IT"),
        "{text}"
    );
    assert!(text.contains("calls it directly"), "{text}");

    let tested = project(&[
        ("shout", extern_wrapper("tg_println")),
        ("test_shout", calls("shout")),
    ]);
    let report = analyze_default(&tested, "shout").unwrap();
    let text = render_human(&report, &PathBuf::from("f.tg"));
    assert!(!text.contains("ASSERTABLE, AND NOTHING"), "{text}");
    assert!(text.contains("reached by 1 test(s): test_shout"), "{text}");
}

#[test]
fn the_human_report_names_the_file_it_was_taken_from() {
    // The path is the only thing distinguishing two otherwise identical
    // sections in a before/after comparison across entry files.
    let globals = project(&[("main", Term::Unit)]);
    let report = analyze_default(&globals, "main").unwrap();
    let text = render_human(&report, &PathBuf::from("src/compiler/main.tg"));

    assert!(
        text.starts_with("Externs reachable from `main` (src/compiler/main.tg):"),
        "{text}"
    );
}
