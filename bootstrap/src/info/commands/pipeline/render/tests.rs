//! Rendering tests for the `tungsten info pipeline` inventory.
//!
//! Split from `render/mod.rs` to keep it under the file-size gate.

use super::*;
use crate::info::commands::pipeline::entry::CostTier;

const ELABORATE_SECTION: Section = Section {
    title: "Info commands",
    cost_hint: "cost 3 — parse + elaborate",
    default_cost: Some(CostTier::Elaborate),
    entries: &[],
};

fn render_one(entry: PipelineEntry, section: &Section) -> String {
    let mut out = String::new();
    let is_flag_table = !entry.group.is_empty();
    render_entry(&mut out, &entry, section, is_flag_table);
    out
}

#[test]
fn short_usage_puts_summary_on_the_same_line_at_the_summary_column() {
    let out = render_one(
        PipelineEntry::subcommand("info def", "tungsten info def <name> <file>", "Show a def")
            .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    assert_eq!(out.lines().count(), 1);
    let line = out.lines().next().unwrap();
    assert_eq!(line.find("Show a def"), Some(SUMMARY_COL));
}

#[test]
fn long_usage_wraps_the_summary_onto_the_next_line() {
    let out = render_one(
        PipelineEntry::subcommand(
            "doctor check unit-cost",
            "tungsten doctor check unit-cost <file> [--threshold 0.5s|8GB] [--json]",
            "Ranked per-unit codegen cost census",
        )
        .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].ends_with("[--json]"));
    assert_eq!(lines[1].find("Ranked"), Some(SUMMARY_COL));
}

#[test]
fn continuation_lines_are_indented_to_the_summary_column() {
    let out = render_one(
        PipelineEntry::subcommand("info def", "tungsten info def <n> <f>", "First\nSecond")
            .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].find("Second"), Some(SUMMARY_COL));
}

#[test]
fn a_tier_matching_the_section_default_is_left_implicit() {
    let out = render_one(
        PipelineEntry::subcommand("info def", "tungsten info def <n> <f>", "Show a def")
            .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    assert!(!out.contains("[cost"), "redundant tier marker: {out}");
}

#[test]
fn a_tier_differing_from_the_section_default_is_annotated() {
    let out = render_one(
        PipelineEntry::subcommand("info pipeline", "tungsten info pipeline", "This message")
            .with_cost(CostTier::Instant),
        &ELABORATE_SECTION,
    );
    assert!(out.contains("[cost 1]"), "missing tier marker: {out}");
}

#[test]
fn cost_and_codegen_markers_combine_into_one_bracket() {
    let out = render_one(
        PipelineEntry::subcommand("info type lowering", "tungsten info type lowering", "L")
            .with_cost(CostTier::Compile)
            .requiring_codegen(),
        &ELABORATE_SECTION,
    );
    assert!(
        out.contains("[cost 4, requires codegen]"),
        "unexpected marker: {out}"
    );
}

#[test]
fn codegen_marker_stands_alone_when_the_tier_is_the_section_default() {
    let out = render_one(
        PipelineEntry::subcommand("info codegen units", "tungsten info codegen units <f>", "U")
            .with_cost(CostTier::Elaborate)
            .requiring_codegen(),
        &ELABORATE_SECTION,
    );
    assert!(
        out.contains("[requires codegen]") && !out.contains("cost"),
        "unexpected marker: {out}"
    );
}

#[test]
fn the_marker_lands_on_the_last_summary_line() {
    let out = render_one(
        PipelineEntry::subcommand("info def", "tungsten info def <n> <f>", "First\nSecond")
            .with_cost(CostTier::Instant),
        &ELABORATE_SECTION,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert!(!lines[0].contains("[cost 1]"));
    assert!(lines[1].ends_with("[cost 1]"), "got {:?}", lines[1]);
}

#[test]
fn see_also_targets_render_on_their_own_line() {
    let out = render_one(
        PipelineEntry::subcommand("doctor check tco-coverage", "tungsten doctor …", "Rank")
            .with_cost(CostTier::Elaborate)
            .with_see_also(&["info codegen musttail-eligibility"]),
        &ELABORATE_SECTION,
    );
    let last = out.lines().last().unwrap();
    assert_eq!(
        last.trim(),
        "See also: info codegen musttail-eligibility",
        "got {last:?}"
    );
}

#[test]
fn a_flag_table_row_places_the_stage_label_in_the_left_column() {
    let section = Section {
        title: "Diagnostic flags",
        cost_hint: "",
        default_cost: None,
        entries: &[],
    };
    let out = render_one(
        PipelineEntry::compile_flag("--dump-types", "Show all type definitions")
            .in_flag_group("Elaborate:"),
        &section,
    );
    let line = out.lines().next().unwrap();
    assert_eq!(line.find("Elaborate:"), Some(2));
    assert_eq!(line.find("--dump-types"), Some(2 + FLAG_GROUP_WIDTH));
    assert_eq!(line.find("Show all"), Some(SUMMARY_COL));
}

#[test]
fn a_prose_note_renders_verbatim_with_no_summary_column() {
    let out = render_one(
        PipelineEntry::note("  Enriched error types:\n    - Argument type mismatch"),
        &ELABORATE_SECTION,
    );
    assert_eq!(
        out,
        "  Enriched error types:\n    - Argument type mismatch\n"
    );
}

#[test]
fn a_heading_carries_its_cost_hint_in_brackets() {
    let section = Section {
        entries: &[],
        ..ELABORATE_SECTION
    };
    let mut out = String::new();
    render_section(&mut out, &section);
    assert_eq!(out, "Info commands [cost 3 — parse + elaborate]:\n");
}

#[test]
fn a_heading_without_a_cost_hint_omits_the_brackets() {
    let section = Section {
        title: "Cross-file diagnostic enrichment (ADR 15.5.26a)",
        cost_hint: "",
        default_cost: None,
        entries: &[],
    };
    let mut out = String::new();
    render_section(&mut out, &section);
    assert_eq!(out, "Cross-file diagnostic enrichment (ADR 15.5.26a):\n");
}

/// A usage string of exactly `width` characters, so a test can sit on the
/// fits-inline / wraps boundary without hand-counting a fixture.
fn usage_of_width(width: usize) -> &'static str {
    Box::leak("u".repeat(width).into_boxed_str())
}

#[test]
fn a_usage_reaching_the_summary_column_wraps_rather_than_touching_the_summary() {
    // `  {usage}` is exactly SUMMARY_COL wide: the first width at which the
    // summary can no longer share the line without abutting the usage.
    let out = render_one(
        PipelineEntry::subcommand("x", usage_of_width(SUMMARY_COL - 2), "Summary")
            .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    assert_eq!(out.lines().count(), 2, "should wrap: {out:?}");
}

#[test]
fn a_usage_one_short_of_the_summary_column_still_fits_inline() {
    let out = render_one(
        PipelineEntry::subcommand("x", usage_of_width(SUMMARY_COL - 3), "Summary")
            .with_cost(CostTier::Elaborate),
        &ELABORATE_SECTION,
    );
    assert_eq!(out.lines().count(), 1, "should fit inline: {out:?}");
    assert_eq!(
        out.lines().next().unwrap().find("Summary"),
        Some(SUMMARY_COL)
    );
}

#[test]
fn a_note_carrying_a_usage_renders_as_a_row_not_as_prose() {
    // Verbatim rendering is for prose blocks: a Note WITH a usage still has
    // a left column, and a non-Note without one still has a summary column.
    let noted_row = PipelineEntry {
        usage: "make devcontainer-profile",
        ..PipelineEntry::note("Orchestrate build + capture")
    };
    let out = render_one(noted_row, &ELABORATE_SECTION);
    let line = out.lines().next().unwrap();
    assert_eq!(line.find("make devcontainer-profile"), Some(2), "{out:?}");
    assert_eq!(line.find("Orchestrate"), Some(SUMMARY_COL), "{out:?}");
}

#[test]
fn a_non_note_without_a_usage_still_gets_the_summary_column() {
    let out = render_one(
        PipelineEntry::compile_flag("", "Summary with no flag spelled"),
        &ELABORATE_SECTION,
    );
    let line = out.lines().next().unwrap();
    assert_eq!(line.find("Summary"), Some(SUMMARY_COL), "{out:?}");
}

const UNGROUPED_ENTRIES: &[PipelineEntry] =
    &[
        PipelineEntry::subcommand("info def", "tungsten info def <n> <f>", "Show a def")
            .with_cost(CostTier::Elaborate),
    ];

const GROUPED_ENTRIES: &[PipelineEntry] =
    &[
        PipelineEntry::compile_flag("--dump-types", "Show all type definitions")
            .in_flag_group("Elaborate:"),
    ];

#[test]
fn a_section_with_no_grouped_entry_renders_without_the_stage_column() {
    let section = Section {
        entries: UNGROUPED_ENTRIES,
        ..ELABORATE_SECTION
    };
    let mut out = String::new();
    render_section(&mut out, &section);
    let row = out.lines().nth(1).expect("heading then one row");
    assert_eq!(
        row.find("tungsten info def"),
        Some(2),
        "an ungrouped section must not reserve the stage column: {row:?}"
    );
}

#[test]
fn a_section_with_any_grouped_entry_reserves_the_stage_column() {
    let section = Section {
        entries: GROUPED_ENTRIES,
        ..ELABORATE_SECTION
    };
    let mut out = String::new();
    render_section(&mut out, &section);
    let row = out.lines().nth(1).expect("heading then one row");
    assert_eq!(row.find("Elaborate:"), Some(2), "{row:?}");
    assert_eq!(
        row.find("--dump-types"),
        Some(2 + FLAG_GROUP_WIDTH),
        "{row:?}"
    );
}

const BANNER_NOTE: &[PipelineEntry] = &[PipelineEntry::note("Tungsten Compiler Pipeline")];
const NOTE_A: &[PipelineEntry] = &[PipelineEntry::note("a")];
const NOTE_B: &[PipelineEntry] = &[PipelineEntry::note("b")];

#[test]
fn an_empty_title_renders_no_heading_line() {
    let section = Section {
        title: "",
        cost_hint: "",
        default_cost: None,
        entries: BANNER_NOTE,
    };
    let mut out = String::new();
    render_section(&mut out, &section);
    assert_eq!(out, "Tungsten Compiler Pipeline\n");
}

#[test]
fn sections_are_separated_by_a_blank_line_and_the_footer_closes_the_listing() {
    let sections = [
        Section {
            title: "First",
            cost_hint: "",
            default_cost: None,
            entries: NOTE_A,
        },
        Section {
            title: "Second",
            cost_hint: "",
            default_cost: None,
            entries: NOTE_B,
        },
    ];
    let out = render(&sections);
    assert!(out.starts_with("First:\na\n\nSecond:\nb\n"), "got {out:?}");
    assert!(out.ends_with("escalate as needed.\n"), "got {out:?}");
    assert!(out.contains("Cost scale: 1=instant"));
}
