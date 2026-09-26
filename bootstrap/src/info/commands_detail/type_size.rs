//! `tungsten info type size` — stored-`Type`-tree size metrics
//! (ADR 8.7.26a §2.3).
//!
//! Where `info type encoding` shows the encoding *shape* (display form),
//! this shows how big the stored tree actually is: total node count, depth,
//! μ-binder nesting chain, and the α-occurrence count per binder — the kᵢ
//! factors of the ADR 7.7.26k unfold estimate ∏ kᵢ. Cost 3 (elaboration
//! only; the walk is a counter over data the elaborator already holds).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_core::diagnostics::type_size::{count_nodes, measure_type};

use crate::info::elaborate_for_info;

use super::diagnostic::print_type_not_found;

/// Display size metrics for a named type's stored tree.
pub fn cmd_info_type_size(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let is_known = project.adt_types.contains_key(name)
        || project.record_types.contains_key(name)
        || project.type_aliases.contains_key(name);
    if !is_known {
        print_type_not_found(name, &project);
        return ExitCode::FAILURE;
    }

    print!("{}", render_type_size_report(name, &project));
    ExitCode::SUCCESS
}

/// Build the full report — separated from printing so tests can assert the
/// exact output (golden-style) on fixtures with known counts.
fn render_type_size_report(name: &str, project: &ProjectOutput) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "Type Size: {name}");
    let _ = writeln!(report, "{}", "\u{2550}".repeat(11 + name.len()));
    let _ = writeln!(report);
    render_encoded_tree_metrics(&mut report, name, project);
    render_per_variant_counts(&mut report, name, project);
    let _ = writeln!(
        report,
        "(shape rather than size: `tungsten info type encoding {name} <file>`)"
    );
    report
}

/// Metrics for the cached (Encoding Finalization) encoding tree, when one exists.
fn render_encoded_tree_metrics(report: &mut String, name: &str, project: &ProjectOutput) {
    let Some(encoded) = project.encoded_types.get(name) else {
        let _ = writeln!(
            report,
            "Stored encoding: (none cached — parameterized types encode per instantiation)"
        );
        let _ = writeln!(report);
        return;
    };
    let metrics = measure_type(encoded);
    let _ = writeln!(report, "Stored encoding tree:");
    let _ = writeln!(report, "  node count: {}", metrics.node_count);
    let _ = writeln!(report, "  max depth:  {}", metrics.max_depth);
    if metrics.mu_binder_chain.is_empty() {
        let _ = writeln!(report, "  \u{3bc}-binders:  (none — not recursive)");
    } else {
        let _ = writeln!(
            report,
            "  \u{3bc}-binder chain: {}",
            metrics.mu_binder_chain.join(" \u{2192} ")
        );
        let _ = writeln!(
            report,
            "  \u{3b1}-occurrences per binder (the \u{220f} k\u{1d62} factors):"
        );
        for (binder, count) in &metrics.alpha_occurrences {
            let _ = writeln!(report, "    {binder}: {count}");
        }
    }
    let _ = writeln!(report);
}

/// Per-variant stored-payload node counts for ADTs (constructor field trees).
fn render_per_variant_counts(report: &mut String, name: &str, project: &ProjectOutput) {
    let Some((_, constructors)) = project.adt_types.get(name) else {
        return;
    };
    let _ = writeln!(report, "Per-variant stored field-tree node counts:");
    for constructor in constructors {
        let field_nodes: usize = constructor.fields.iter().map(count_nodes).sum();
        let _ = writeln!(
            report,
            "  {}: {} node(s) across {} field(s)",
            constructor.name,
            field_nodes,
            constructor.fields.len()
        );
    }
    let _ = writeln!(report);
}

// Tests: type_size_tests.rs
#[cfg(test)]
#[path = "type_size_tests.rs"]
mod type_size_tests;
