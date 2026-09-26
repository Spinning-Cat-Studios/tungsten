//! One elaboration, many roots (ADR 3.9.26c D1 / AC1 / AC2 / AC5).
//!
//! The fan-out is a driver change, so what is assertable here is the driver's
//! three decisions — which roots, in what order, and what shape to print — and
//! the promise that a section taken from a set is the same bytes as one taken
//! alone. The saving itself is wall clock and is measured by hand.

use std::path::PathBuf;

use super::super::render::{render_human, render_human_sections, render_json, render_json_array};
use super::super::{report_shape, resolve_roots, ReportShape};
use super::*;

fn defs(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

#[test]
fn the_single_definition_spelling_resolves_to_exactly_its_one_root() {
    // D3: with no `--defs`, nothing about the invocation changes — including
    // the `--json` object/array choice, which keys on this length.
    assert_eq!(
        resolve_roots("alpha", &defs(&[])),
        vec!["alpha".to_string()]
    );
}

#[test]
fn one_requested_root_prints_the_single_definition_shape() {
    // D3 again, at the other end: this is the decision that keeps `--json` a
    // bare object for every consumer written against the old spelling.
    assert_eq!(report_shape(1, 1), ReportShape::Single);
}

#[test]
fn asking_for_a_set_prints_a_set_even_when_a_name_was_misspelled() {
    // Keyed on the REQUEST, not on how many resolved: a consumer that passed
    // `--defs` and parses an array must not be handed an object because one
    // root was absent.
    assert_eq!(report_shape(2, 2), ReportShape::Set);
    assert_eq!(report_shape(3, 1), ReportShape::Set);
}

#[test]
fn no_resolved_root_prints_nothing_rather_than_an_empty_answer() {
    // An empty array or an empty section would read as "walked them, found
    // nothing" — the one thing this command exists never to say by accident.
    assert_eq!(report_shape(1, 0), ReportShape::Nothing);
    assert_eq!(report_shape(3, 0), ReportShape::Nothing);
}

#[test]
fn a_comma_separated_defs_value_and_repeated_flags_mean_the_same_thing() {
    let joined = resolve_roots("alpha", &defs(&["bravo,charlie"]));
    let repeated = resolve_roots("alpha", &defs(&["bravo", "charlie"]));

    assert_eq!(joined, repeated);
    assert_eq!(
        joined,
        vec![
            "alpha".to_string(),
            "bravo".to_string(),
            "charlie".to_string()
        ]
    );
}

#[test]
fn roots_keep_request_order_so_a_before_after_diff_lines_up() {
    // Sorting here would silently reorder the sections between two runs that
    // named the same definitions differently — the exact comparison AC2 exists
    // to make valid.
    assert_eq!(
        resolve_roots("zulu", &defs(&["alpha", "mike"])),
        vec!["zulu".to_string(), "alpha".to_string(), "mike".to_string()]
    );
}

#[test]
fn a_duplicate_root_is_walked_once_however_it_was_spelled() {
    assert_eq!(
        resolve_roots("alpha", &defs(&["alpha", "bravo,alpha", " alpha "])),
        vec!["alpha".to_string(), "bravo".to_string()]
    );
}

#[test]
fn blank_and_padded_defs_entries_are_dropped_rather_than_walked() {
    // `--defs a,,b` and a trailing comma are ordinary typing accidents; walking
    // `""` would report a spurious "no definition named ''".
    assert_eq!(
        resolve_roots("alpha", &defs(&["bravo,,", " charlie "])),
        vec![
            "alpha".to_string(),
            "bravo".to_string(),
            "charlie".to_string()
        ]
    );
}

#[test]
fn one_shared_test_index_gives_every_root_the_answer_it_would_get_alone() {
    // The substance of D1: the fan-out reuses one elaboration AND one `test_*`
    // scan, so this asserts the sharing is not a behaviour change.
    let globals = project(&[
        ("shout", extern_wrapper("tg_println")),
        ("joins", extern_wrapper("tg_path_join")),
        ("quiet", Term::Unit),
        ("test_shout", calls("shout")),
    ]);
    let shared = TestReferences::index(&globals);

    for root in ["shout", "joins", "quiet"] {
        let fanned = analyze(&globals, root, DEFAULT_MAX_VISITED, &shared).unwrap();
        assert_eq!(fanned, analyze_default(&globals, root).unwrap(), "{root}");
    }
}

#[test]
fn one_root_renders_byte_identically_through_the_set_renderer() {
    // AC2 in its strictest form: the set spelling must not add a header, a
    // separator or an index to a section a reader will diff against the single
    // spelling's output.
    let globals = project(&[("shout", extern_wrapper("tg_println"))]);
    let report = analyze_default(&globals, "shout").unwrap();
    let file = PathBuf::from("m.tg");

    assert_eq!(
        render_human_sections(std::slice::from_ref(&report), &file),
        render_human(&report, &file)
    );
    assert_eq!(
        render_json_array(&[report.clone()]),
        format!("[\n{}\n]", render_json(&report))
    );
}

#[test]
fn several_sections_appear_in_request_order_separated_by_a_blank_line() {
    let globals = project(&[
        ("shout", extern_wrapper("tg_println")),
        ("joins", extern_wrapper("tg_path_join")),
    ]);
    let file = PathBuf::from("m.tg");
    let reports = vec![
        analyze_default(&globals, "joins").unwrap(),
        analyze_default(&globals, "shout").unwrap(),
    ];
    let text = render_human_sections(&reports, &file);

    assert_eq!(
        text,
        format!(
            "{}\n{}",
            render_human(&reports[0], &file),
            render_human(&reports[1], &file)
        )
    );
    assert!(
        text.find("from `joins`").unwrap() < text.find("from `shout`").unwrap(),
        "{text}"
    );
}

#[test]
fn every_element_of_the_json_array_is_what_the_single_form_emits() {
    let globals = project(&[
        ("shout", extern_wrapper("tg_println")),
        ("joins", extern_wrapper("tg_path_join")),
    ]);
    let reports = vec![
        analyze_default(&globals, "shout").unwrap(),
        analyze_default(&globals, "joins").unwrap(),
    ];
    let json = render_json_array(&reports);

    for report in &reports {
        assert!(json.contains(&render_json(report)), "{json}");
    }
    assert!(json.starts_with("[\n"), "{json}");
    assert!(json.ends_with("\n]"), "{json}");
    assert!(
        json.contains("},\n{"),
        "elements must be comma-separated: {json}"
    );
}
