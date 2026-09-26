//! ADR 20.8.26c AC6: `info builtins`' rows, verdicts, and its empty-input finding.

use super::{render_listing, render_one, report, rows, Row};

fn row(name: &str, bootstrap: bool, selfhost: bool, declared: Option<&'static str>) -> Row {
    Row {
        name: name.to_string(),
        bootstrap,
        selfhost,
        declared,
    }
}

#[test]
fn a_symmetric_row_reads_both() {
    assert_eq!(row("compare", true, true, None).verdict(), "both");
}

#[test]
fn a_declared_asymmetry_names_its_slug() {
    let verdict = row("char_at", true, false, Some("no-tg-fallback")).verdict();
    assert!(verdict.contains("ASYMMETRIC"), "{verdict}");
    assert!(verdict.contains("no-tg-fallback"), "{verdict}");
}

#[test]
fn an_undeclared_asymmetry_says_so_rather_than_naming_a_slug() {
    let verdict = row("newcomer", false, true, None).verdict();
    assert!(verdict.contains("UNDECLARED"), "{verdict}");
}

/// AC6's explicit requirement: the empty case is a finding, not a blank.
#[test]
fn an_empty_union_renders_as_a_fault_not_as_zero() {
    let empty = render_listing(&[], 0, 0);
    assert!(empty.contains("FAULT"), "{empty}");
    assert!(!empty.contains("0 asymmetry/ies"), "{empty}");
}

/// The tick column is the listing's whole point — which compiler carries which
/// name — and it is the one cell a reader scans rather than reads.
#[test]
fn the_tick_column_distinguishes_the_two_compilers_per_row() {
    let out = render_listing(&[row("char_at", true, false, Some("no-tg-fallback"))], 1, 0);
    let line = out
        .lines()
        .find(|l| l.contains("char_at"))
        .expect("the row is rendered");
    assert_eq!(line.matches('✓').count(), 1, "{line}");
    assert_eq!(line.matches('·').count(), 1, "{line}");
    // Column order is the claim: bootstrap first. Swapped, the row would say
    // the opposite of the truth while still containing both marks.
    assert!(
        line.find('✓') < line.find('·'),
        "the bootstrap column is not first: {line}"
    );
}

#[test]
fn a_row_in_both_tables_renders_two_present_marks_and_no_absent_one() {
    let out = render_listing(&[row("compare", true, true, None)], 1, 1);
    let line = out
        .lines()
        .find(|l| l.contains("compare"))
        .expect("the row is rendered");
    assert_eq!(line.matches('✓').count(), 2, "{line}");
    assert_eq!(line.matches('·').count(), 0, "{line}");
}

#[test]
fn a_populated_listing_reports_both_counts_separately() {
    let out = render_listing(
        &[
            row("compare", true, true, None),
            row("char_at", true, false, Some("no-tg-fallback")),
            row("newcomer", false, true, None),
        ],
        2,
        2,
    );
    assert!(out.contains("reach: 3 name(s)"), "{out}");
    assert!(
        out.contains("2 asymmetry/ies, 1 of them undeclared."),
        "{out}"
    );
    assert!(!out.contains("FAULT"), "{out}");
}

#[test]
fn the_shipped_rows_mark_substring_as_intercepted_on_both_sides() {
    let shipped = rows();
    let substring = shipped
        .iter()
        .find(|r| r.name == "substring")
        .expect("substring is intercepted");
    assert!(substring.bootstrap);
    assert!(substring.selfhost);
    assert!(!substring.is_asymmetric());
}

#[test]
fn every_shipped_asymmetry_is_declared() {
    let undeclared: Vec<String> = rows()
        .into_iter()
        .filter(|r| r.is_asymmetric() && r.declared.is_none())
        .map(|r| r.name)
        .collect();
    assert!(undeclared.is_empty(), "undeclared: {undeclared:?}");
}

#[test]
fn a_name_neither_compiler_intercepts_gets_an_answer_not_an_error() {
    let out = render_one("lex_slice", &rows(), None);
    assert!(out.contains("not intercepted by either"), "{out}");
    assert!(out.contains("that is an answer, not an error"), "{out}");
}

/// AC6 is about the **default** — what running the command with no arguments
/// does — and a default's consequence is what `--help` cannot show.
#[test]
fn the_no_argument_default_prints_the_whole_listing() {
    let out = report(None);
    // 10 → 12 when ADR 14.9.26c added `to_int` / `from_int` to both tables.
    assert!(out.contains("reach: 12 name(s) across two tables"), "{out}");
    assert!(out.contains("substring"), "{out}");
    assert!(out.contains("to_int"), "{out}");
    assert!(out.contains("0 of them undeclared."), "{out}");
    assert!(
        !out.contains("Builtin: "),
        "single-name header in the listing: {out}"
    );
}

#[test]
fn an_argument_selects_the_single_name_report_instead() {
    let out = report(Some("substring"));
    assert!(out.contains("Builtin: substring"), "{out}");
    assert!(
        !out.contains("reach:"),
        "listing header in a single-name report: {out}"
    );
}

#[test]
fn the_single_name_report_carries_the_interception_note() {
    let out = render_one(
        "char_at",
        &rows(),
        Some("the note as the predicate wrote it".to_string()),
    );
    assert!(out.contains("Builtin: char_at"), "{out}");
    assert!(out.contains("the note as the predicate wrote it"), "{out}");
}
