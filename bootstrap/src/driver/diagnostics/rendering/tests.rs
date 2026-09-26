//! Tests for the diagnostic rendering pipeline — split from `mod.rs`
//! (ADR 14.8.26g: D7's truncation tests took the file over the size limit).

use super::*;
use crate::elaborate::ElabErrorKind;
use crate::span::Span;

fn elab_error(start: u32, end: u32) -> ElabError {
    ElabError::new(
        Span::new(start, end),
        ElabErrorKind::UndefinedVariable("x".to_string()),
    )
}

fn parse_err(start: u32, end: u32) -> ParseError {
    ParseError::new(
        Span::new(start, end),
        crate::error::ParseErrorKind::UnexpectedEof,
    )
}

fn src() -> SourceRef<'static> {
    SourceRef {
        source: "fn f() -> Nat { x }",
        filename: "t.tg",
    }
}

// ─────────────────────────────────────────────────────────────────────
// display_budget
// ─────────────────────────────────────────────────────────────────────

#[test]
fn zero_max_errors_means_no_limit() {
    assert_eq!(display_budget(7, 0), (7, 0));
}

#[test]
fn a_limit_below_the_total_withholds_the_remainder() {
    assert_eq!(display_budget(7, 3), (3, 4));
}

#[test]
fn a_limit_above_the_total_withholds_nothing() {
    assert_eq!(display_budget(2, 10), (10, 0));
}

#[test]
fn a_limit_equal_to_the_total_withholds_nothing() {
    assert_eq!(display_budget(4, 4), (4, 0));
}

#[test]
fn no_diagnostics_needs_no_budget() {
    assert_eq!(display_budget(0, 0), (0, 0));
}

// ─────────────────────────────────────────────────────────────────────
// split_budget
// ─────────────────────────────────────────────────────────────────────

#[test]
fn an_ample_budget_renders_everything() {
    assert_eq!(split_budget(2, 3, 5), RenderPlan { parse: 2, elab: 3 });
}

/// Parse errors render first, so they take the budget first — an elab-first
/// split would show the wrong diagnostics under `--max-errors=1`.
#[test]
fn parse_errors_consume_the_budget_first() {
    assert_eq!(split_budget(2, 3, 1), RenderPlan { parse: 1, elab: 0 });
}

#[test]
fn elaboration_errors_get_what_parse_errors_leave() {
    assert_eq!(split_budget(2, 3, 4), RenderPlan { parse: 2, elab: 2 });
}

#[test]
fn a_zero_budget_renders_nothing() {
    assert_eq!(split_budget(2, 3, 0), RenderPlan { parse: 0, elab: 0 });
}

#[test]
fn no_parse_errors_leaves_the_whole_budget_to_elaboration() {
    assert_eq!(split_budget(0, 3, 2), RenderPlan { parse: 0, elab: 2 });
}

// ─────────────────────────────────────────────────────────────────────
// raw_diagnostic_count
// ─────────────────────────────────────────────────────────────────────

#[test]
fn the_raw_count_sums_both_kinds() {
    assert_eq!(
        raw_diagnostic_count(&[parse_err(0, 1), parse_err(2, 3)], &[elab_error(4, 5)]),
        3
    );
}

#[test]
fn the_raw_count_of_nothing_is_zero() {
    assert_eq!(raw_diagnostic_count(&[], &[]), 0);
}

/// It counts *raw* diagnostics — two errors at one span are two, even
/// though deduplication will fold them to one.
#[test]
fn the_raw_count_precedes_deduplication() {
    let same_span = [elab_error(0, 5), elab_error(0, 5)];
    assert_eq!(raw_diagnostic_count(&[], &same_span), 2);
    assert_eq!(deduplicate_errors(&same_span).len(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// truncation_plan (ADR 14.8.26g D7)
// ─────────────────────────────────────────────────────────────────────

fn error_in_file(file: &str) -> ElabError {
    elab_error(0, 1).with_file_path(std::path::PathBuf::from(file))
}

#[test]
fn an_ample_budget_selects_everything_in_order() {
    let errors = [error_in_file("a.tg"), error_in_file("b.tg")];
    let refs: Vec<&ElabError> = errors.iter().collect();
    assert_eq!(truncation_plan(&refs, 5), vec![0, 1]);
}

/// Truncation keeps at least one diagnostic per failing file: five errors
/// in `a.tg` followed by one in `b.tg`, budget 2 → the first of each,
/// not the first two of `a.tg`.
#[test]
fn truncation_keeps_one_diagnostic_per_failing_file() {
    let errors = [
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("b.tg"),
    ];
    let refs: Vec<&ElabError> = errors.iter().collect();
    assert_eq!(truncation_plan(&refs, 2), vec![0, 5]);
}

/// Leftover budget after the per-file pass fills in list order.
#[test]
fn leftover_budget_fills_in_list_order() {
    let errors = [
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("a.tg"),
        error_in_file("b.tg"),
    ];
    let refs: Vec<&ElabError> = errors.iter().collect();
    assert_eq!(truncation_plan(&refs, 3), vec![0, 1, 3]);
}

/// More failing files than budget: the first `limit` files get one each —
/// no selection can do better.
#[test]
fn more_files_than_budget_takes_the_first_files() {
    let errors = [
        error_in_file("a.tg"),
        error_in_file("b.tg"),
        error_in_file("c.tg"),
    ];
    let refs: Vec<&ElabError> = errors.iter().collect();
    assert_eq!(truncation_plan(&refs, 2), vec![0, 1]);
}

// ─────────────────────────────────────────────────────────────────────
// the render entry points' verdict
//
// These write to stderr; what is asserted is only the returned
// "were there errors" verdict, which is what the driver's exit code reads.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rendering_nothing_reports_no_errors() {
    assert!(!render_diagnostics_limited(&src(), &[], &[], &[], 0));
}

#[test]
fn rendering_an_elab_error_reports_errors() {
    assert!(render_diagnostics_limited(
        &src(),
        &[elab_error(16, 17)],
        &[],
        &[],
        0
    ));
}

#[test]
fn rendering_a_parse_error_reports_errors() {
    assert!(render_diagnostics_limited(
        &src(),
        &[],
        &[parse_err(16, 17)],
        &[],
        0
    ));
}

#[test]
fn rendering_only_warnings_reports_no_errors() {
    assert!(!render_diagnostics_limited(
        &src(),
        &[],
        &[],
        &[elab_error(16, 17)],
        0
    ));
}

#[test]
fn the_source_map_entry_point_reports_no_errors_when_there_are_none() {
    let map = crate::driver::modules::SourceMap::default();
    assert!(!render_diagnostics_with_source_map_limited(
        &src(),
        &map,
        &[],
        &[],
        0
    ));
}

#[test]
fn the_source_map_entry_point_reports_errors_when_there_are_some() {
    let map = crate::driver::modules::SourceMap::default();
    assert!(render_diagnostics_with_source_map_limited(
        &src(),
        &map,
        &[elab_error(16, 17)],
        &[],
        0
    ));
}
