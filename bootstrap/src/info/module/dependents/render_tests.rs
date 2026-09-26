//! Tests for [`super::render`] (ADR 5.9.26f).
//!
//! Pure over a [`super::DependentsReport`], so the shape of the answer — the
//! three sections, the per-hop grouping, and the reach line that keeps
//! `0 dependents` apart from `0 modules read` — is assertable with no tree.

use super::render::render_report;
use super::tests::report_for_fixture;
use super::DependentsReport;

/// 5.9.26f AC3 — a tree with no dependents renders differently from a tree
/// nothing was read from.
#[test]
fn test_reach_line_distinguishes_empty_tree_from_no_dependents() {
    let no_dependents = DependentsReport {
        module: "a::b".to_string(),
        modules_in_tree: 12,
        sites_examined: 30,
        ..DependentsReport::default()
    };
    let nothing_read = DependentsReport {
        module: "a::b".to_string(),
        ..DependentsReport::default()
    };

    let clean = render_report(&no_dependents, false);
    let fault = render_report(&nothing_read, false);

    assert!(clean.contains("direct (0)"), "{clean}");
    assert!(
        clean.contains("reach: 12 module(s) in tree, 30 import site(s) examined, 0 unresolved"),
        "{clean}"
    );
    assert!(
        fault.contains("no modules read") && fault.contains("FAULT"),
        "{fault}"
    );
    assert!(
        !fault.contains("reach:") && !fault.contains("direct (0)"),
        "an unread tree must not render as a clean zero: {fault}"
    );
    assert_ne!(clean, fault);
}

/// 5.9.26f AC1 — the indirect section groups by re-export hop, and only
/// `--verbose` spells the sites out.
#[test]
fn test_indirect_sites_are_grouped_until_verbose() {
    let report = report_for_fixture();

    let terse = render_report(&report, false);
    assert!(terse.contains("via driver::ffi — 2 site(s)"), "{terse}");
    assert!(terse.contains("--verbose to list the sites"), "{terse}");
    assert!(!terse.contains("far.tg:1"), "{terse}");

    let loud = render_report(&report, true);
    assert!(loud.contains("far.tg:1"), "{loud}");
    assert!(!loud.contains("--verbose to list the sites"), "{loud}");
}

/// 5.9.26f AC2 — every section of the report renders, with its rows.
///
/// The reach line alone cannot say this: a section that silently rendered
/// nothing would leave the counts intact and the answer empty.
#[test]
fn test_every_section_renders_its_rows() {
    let report = report_for_fixture();
    let rendered = render_report(&report, false);

    assert!(
        rendered.contains(&format!("direct ({})", report.direct.len())),
        "{rendered}"
    );
    assert!(
        rendered.contains("pub use driver::ffi::types::*"),
        "the direct section lists its sites, not just their count: {rendered}"
    );
    assert!(
        rendered.contains(&format!("literals ({})", report.literals.len())),
        "{rendered}"
    );
    assert!(
        rendered.contains("driver::ffi::types::emit"),
        "the literals section lists the matched text: {rendered}"
    );
    assert!(rendered.contains("via driver::ffi"), "{rendered}");
}
