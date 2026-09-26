//! Rendering for `tungsten doctor check comparable`.
//!
//! Split from the analysis so the report text is asserted by unit tests without
//! capturing stdout, and so each defect class gets its own wording — they have
//! different fixes, so they must read differently.

use std::fmt::Write as _;

use tungsten_core::eval::ComparatorFailureKind;

use super::ComparabilityReport;
use crate::comparator::gate::CLOSURE_CAP;

/// Render the report for one type.
///
/// A clean verdict is one line; a failing one names the class, the fix, and the
/// consequence — a reader who is told only "not comparable" learns nothing
/// about which of three unrelated problems they have.
pub(crate) fn render_report(name: &str, report: &ComparabilityReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Comparability: {name}");
    let _ = writeln!(out, "{}", "\u{2550}".repeat(15 + name.len()));
    let _ = writeln!(out);

    let Some(failure) = &report.failure else {
        let _ = writeln!(
            out,
            "✓ comparable — {} comparator(s) synthesized, closure complete",
            report.closure_size
        );
        return out;
    };

    render_failure(&mut out, name, failure);
    render_consequence(&mut out);
    out
}

fn render_failure(out: &mut String, name: &str, failure: &ComparatorFailureKind) {
    match failure {
        ComparatorFailureKind::OpaqueLeaf { path } => {
            let _ = writeln!(out, "✗ opaque leaf: {path}");
            let _ = writeln!(
                out,
                "  This type is noncomparable BY POLICY (ADR 29.6.26f §2.2), not by defect."
            );
            let _ = writeln!(
                out,
                "  Compare a projection of it, or normalize the field away first."
            );
        }
        ComparatorFailureKind::EmptyClosure => {
            let _ = writeln!(out, "✗ no comparator could be synthesized for this type");
        }
        ComparatorFailureKind::IncompleteClosure { dangling, cause } => {
            render_incomplete_closure(out, dangling, cause.as_deref());
        }
        ComparatorFailureKind::LimitExceeded { bound } => {
            let _ = writeln!(
                out,
                "✗ synthesis did not converge within {bound} comparators (cap {CLOSURE_CAP})"
            );
            let _ = writeln!(
                out,
                "  Check the μ-unfold factor with `tungsten info type size {name} <file>`."
            );
        }
        ComparatorFailureKind::NoSynthesizer => {
            let _ = writeln!(
                out,
                "✗ no comparator synthesizer is installed on this environment"
            );
            let _ = writeln!(
                out,
                "  This is a compiler wiring fault, not a fact about the type."
            );
        }
    }
    let _ = writeln!(out);
}

/// The dangling-symbol class, which needs two paragraphs rather than one: the
/// symbol names the shape that could not be built, and the cause names why —
/// and they are usually in different parts of the type.
fn render_incomplete_closure(out: &mut String, dangling: &str, cause: Option<&str>) {
    let _ = writeln!(
        out,
        "✗ the synthesized comparator calls `{dangling}`, which is never defined"
    );
    let Some(cause) = cause else {
        let _ = writeln!(
            out,
            "  The walk could not attribute this to a sub-type it refused, which"
        );
        let _ = writeln!(
            out,
            "  usually means the reference came from a body whose operand type was"
        );
        let _ = writeln!(out, "  never queued (ADR 1.8.26b D2).");
        return;
    };
    let _ = writeln!(out, "  Because: {cause}");
    let _ = writeln!(
        out,
        "  The symbol names the shape that could not be built, not the reason —"
    );
    let _ = writeln!(out, "  fix the field above.");
}

/// The half that gets ignored if it is left out: what a failing verdict MEANS
/// for the tests the reader is about to write.
fn render_consequence(out: &mut String) {
    let _ = writeln!(
        out,
        "A `compare` at this type fails the run (ADR 1.8.26b D3) — it no longer"
    );
    let _ = writeln!(
        out,
        "passes silently. Fix the cause above, or don't assert at this type."
    );
}

/// Render the `--all` summary: one line per type, failures detailed underneath.
///
/// A table first, because the question `--all` answers is "which of these can I
/// assert on?" and that is a scan, not a read. The detail follows only for the
/// failures, so a clean corpus is N lines rather than N sections.
pub(crate) fn render_summary(reports: &[(&str, ComparabilityReport)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Comparability: {} type(s)", reports.len());
    let _ = writeln!(out, "{}", "\u{2550}".repeat(30));
    let _ = writeln!(out);

    let width = reports.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for (name, report) in reports {
        let verdict = match &report.failure {
            None => format!("✓ comparable ({} comparator(s))", report.closure_size),
            Some(kind) => format!("✗ {}", failure_label(kind)),
        };
        let _ = writeln!(out, "  {name:<width$}  {verdict}");
    }

    let failures: Vec<&(&str, ComparabilityReport)> =
        reports.iter().filter(|(_, r)| !r.is_comparable()).collect();
    let _ = writeln!(out);
    if failures.is_empty() {
        let _ = writeln!(
            out,
            "✓ all {} type(s) comparable — an assertion at any of them means something",
            reports.len()
        );
        return out;
    }

    let _ = writeln!(
        out,
        "{} of {} type(s) cannot be compared:",
        failures.len(),
        reports.len()
    );
    let _ = writeln!(out);
    for (name, report) in failures {
        let _ = writeln!(out, "── {name} ──");
        if let Some(kind) = &report.failure {
            render_failure(&mut out, name, kind);
        }
    }
    render_consequence(&mut out);
    out
}

/// The one-word class for the summary table. Deliberately not the full
/// sentence: the table is for scanning, and the sentence is right below it.
pub(super) fn failure_label(kind: &ComparatorFailureKind) -> &'static str {
    match kind {
        ComparatorFailureKind::OpaqueLeaf { .. } => "opaque leaf",
        ComparatorFailureKind::EmptyClosure => "nothing synthesizable",
        ComparatorFailureKind::IncompleteClosure { .. } => "incomplete closure",
        ComparatorFailureKind::LimitExceeded { .. } => "synthesis did not settle",
        ComparatorFailureKind::NoSynthesizer => "no synthesizer installed",
    }
}
