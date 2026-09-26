//! Tests for `doctor check sorry-sites` (ADR 18.9.26g AC1).
//!
//! The rows come from a real elaboration of the golden fixture: the claim is
//! that the elaborator marks authored holes and the lowering leaves its own
//! bare, so a hand-built `ProjectOutput` would assert nothing about either.

use std::process::ExitCode;

use crate::driver;

use super::{census, cmd_check_sorry_sites, render, SorryCensus, SorryRow};

const FIXTURE: &str = include_str!("../../../../tests/golden/check/sorry_counts.tg");

fn fixture_census() -> SorryCensus {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sorry_counts.tg");
    std::fs::write(&path, FIXTURE).unwrap();
    let project = driver::elaborate_project(&path, false, 20, None).expect("fixture elaborates");
    census(&project)
}

fn row<'a>(found: &'a SorryCensus, name: &str) -> &'a SorryRow {
    found
        .rows
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row for {name}: {found:#?}"))
}

/// 1-based line of the first fixture line containing `needle`.
fn fixture_line(needle: &str) -> usize {
    FIXTURE
        .lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("fixture has no line containing {needle}"))
        + 1
}

// 18.9.26g AC1: the authored `sorry` is named at its file:line:col.
#[test]
fn an_authored_sorry_is_named_at_its_line() {
    let found = fixture_census();
    let authored = row(&found, "authored_hole");
    assert_eq!(authored.authored.len(), 1, "{authored:?}");
    assert!(authored.synthesised.is_empty() && authored.unclassified == 0);
    let line = fixture_line("    sorry");
    assert!(
        authored.authored[0].ends_with(&format!("sorry_counts.tg:{line}:5")),
        "{authored:?}"
    );
}

// 18.9.26g AC1: an axiom's hole is authored.
#[test]
fn an_axiom_is_one_authored_hole() {
    let found = fixture_census();
    let axiom = row(&found, "trusted");
    assert_eq!(axiom.authored.len(), 1, "{axiom:?}");
    assert!(axiom.synthesised.is_empty() && axiom.unclassified == 0);
}

// 18.9.26g AC1: `Cons(x, Nil())` on a two-constructor type is an absurd branch only.
#[test]
fn a_nested_two_constructor_pattern_is_an_absurd_branch() {
    let found = fixture_census();
    let head = row(&found, "singleton_head");
    assert!(
        head.authored.is_empty() && head.unclassified == 0,
        "{head:?}"
    );
    assert!(!head.synthesised.is_empty());
    assert!(
        head.synthesised.iter().all(|c| *c == "absurd branch"),
        "{head:?}"
    );
}

// 18.9.26g AC1: a nested three-constructor pattern is an unreachable pattern arm.
#[test]
fn a_nested_three_constructor_pattern_is_an_unreachable_arm() {
    let found = fixture_census();
    let tri = row(&found, "tri_payload");
    assert!(tri.authored.is_empty() && tri.unclassified == 0, "{tri:?}");
    assert!(
        tri.synthesised.contains(&"unreachable pattern arm"),
        "{tri:?}"
    );
}

// 18.9.26g AC1: `==` with no equality primitive plants one unclassified hole.
#[test]
fn equality_without_a_primitive_is_unclassified() {
    let found = fixture_census();
    let eq = row(&found, "same_option");
    assert_eq!(eq.unclassified, 1, "{eq:?}");
    assert!(eq.authored.is_empty() && eq.synthesised.is_empty());
}

// 18.9.26g AC1: one definition can carry both classes; neither hides the other.
#[test]
fn a_mixed_definition_reports_both_classes() {
    let found = fixture_census();
    let mixed = row(&found, "mixed_hole");
    assert_eq!(mixed.authored.len(), 1, "{mixed:?}");
    assert!(!mixed.synthesised.is_empty(), "{mixed:?}");
}

// 18.9.26g AC1: totals sum the rows, and every examined definition is counted.
#[test]
fn totals_sum_the_rows() {
    let found = fixture_census();
    assert_eq!(found.rows.len(), 6, "{found:#?}");
    assert!(found.examined >= found.rows.len());
    let authored: usize = found.rows.iter().map(|r| r.authored.len()).sum();
    let synthesised: usize = found.rows.iter().map(|r| r.synthesised.len()).sum();
    let unclassified: usize = found.rows.iter().map(|r| r.unclassified).sum();
    assert_eq!(
        (found.authored, found.synthesised, found.unclassified),
        (authored, synthesised, unclassified)
    );
}

#[test]
fn the_report_names_each_row_and_the_totals() {
    let found = SorryCensus {
        examined: 4,
        rows: vec![SorryRow {
            name: "f".to_string(),
            authored: vec!["a.tg:2:5".to_string()],
            synthesised: vec!["absurd branch"],
            unclassified: 1,
        }],
        authored: 1,
        synthesised: 1,
        unclassified: 1,
    };
    let text = render(&found, "a.tg");
    assert!(
        text.starts_with("⚠ 1 of 4 definition(s) carry a sorry in a.tg:"),
        "{text}"
    );
    assert!(text.contains("  f\n"), "{text}");
    assert!(text.contains("authored      a.tg:2:5"), "{text}");
    assert!(text.contains("synthesised   absurd branch"), "{text}");
    assert!(text.contains("unclassified  1"), "{text}");
    assert!(
        text.contains("totals: 1 authored, 1 synthesised, 1 unclassified"),
        "{text}"
    );
}

#[test]
fn a_clean_report_says_how_many_it_examined() {
    let found = SorryCensus {
        examined: 0,
        rows: vec![],
        authored: 0,
        synthesised: 0,
        unclassified: 0,
    };
    assert_eq!(
        render(&found, "a.tg"),
        "✓ no definition carries a sorry in a.tg (0 definition(s) examined)\n"
    );
}

#[test]
fn a_row_with_no_unclassified_hole_prints_no_unclassified_line() {
    let found = SorryCensus {
        examined: 1,
        rows: vec![SorryRow {
            name: "g".to_string(),
            authored: vec![],
            synthesised: vec!["unreachable pattern arm"],
            unclassified: 0,
        }],
        authored: 0,
        synthesised: 1,
        unclassified: 0,
    };
    let text = render(&found, "a.tg");
    assert!(!text.contains("unclassified  "), "{text}");
}

// A file that does not elaborate is a failure, not an empty census: findings
// exit 0, so the exit code is the only thing that tells the two apart.
#[test]
fn a_file_that_does_not_elaborate_exits_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.tg");
    assert_eq!(
        cmd_check_sorry_sites(&missing, false, false),
        ExitCode::FAILURE
    );
}

#[test]
fn the_fixture_exits_success_in_both_formats() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sorry_counts.tg");
    std::fs::write(&path, FIXTURE).unwrap();
    assert_eq!(
        cmd_check_sorry_sites(&path, false, false),
        ExitCode::SUCCESS
    );
    assert_eq!(cmd_check_sorry_sites(&path, true, false), ExitCode::SUCCESS);
}
