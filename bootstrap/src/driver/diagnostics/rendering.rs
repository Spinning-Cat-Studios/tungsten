//! Diagnostic rendering pipeline: deduplication, limiting, and summary output.

use super::hints::{self, HintTracker};
use super::renderers::{
    render_elab_error, render_elab_error_with_source_map, render_parse_error, render_warning,
    render_warning_with_source_map,
};
use crate::driver::modules::SourceMap;
use crate::{ElabError, ParseError};

/// Default source text and filename for diagnostic rendering.
pub struct SourceRef<'a> {
    pub source: &'a str,
    pub filename: &'a str,
}

/// Print the error/warning summary footer.
///
/// Handles hint suppression counts, omitted error counts, and warning counts.
/// Returns `true` if there were errors.
fn print_summary(
    hint_tracker: &HintTracker,
    total_errors: usize,
    total_warnings: usize,
    omitted: usize,
) -> bool {
    let (text, has_errors) = format_summary(
        hint_tracker.suppressed_count(),
        total_errors,
        total_warnings,
        omitted,
    );
    if !text.is_empty() {
        eprint!("{}", text);
    }
    has_errors
}

/// Build the summary text. Returns `(text, has_errors)`.
///
/// Pure function extracted from `print_summary` for testability.
fn format_summary(
    suppressed_hints: usize,
    total_errors: usize,
    total_warnings: usize,
    omitted: usize,
) -> (String, bool) {
    let mut out = String::new();

    if total_errors > 0 {
        out.push('\n');
        if suppressed_hints > 0 {
            out.push_str(&format!(
                "  {} additional diagnostic hint{} suppressed (use --verbose-hints to show all)\n",
                suppressed_hints,
                if suppressed_hints == 1 { "" } else { "s" }
            ));
        }
        let warning_suffix = format_warning_suffix(total_warnings);
        if omitted > 0 {
            out.push_str(&format!(
                "error: aborting due to {} error{} ({} not shown; use --max-errors=0 to see all){}\n",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                omitted,
                warning_suffix,
            ));
        } else {
            out.push_str(&format!(
                "error: aborting due to {} error{}{}\n",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                warning_suffix,
            ));
        }
        (out, true)
    } else if total_warnings > 0 {
        out.push('\n');
        out.push_str(&format!(
            "warning: {} warning{} emitted\n",
            total_warnings,
            if total_warnings == 1 { "" } else { "s" }
        ));
        (out, false)
    } else {
        (out, false)
    }
}

/// Format the "; N warning(s) emitted" suffix (empty if no warnings).
fn format_warning_suffix(total_warnings: usize) -> String {
    if total_warnings > 0 {
        format!(
            "; {} warning{} emitted",
            total_warnings,
            if total_warnings == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    }
}

/// Render elaboration errors with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_with_source_map_limited(
    default: &SourceRef<'_>,
    source_map: &SourceMap,
    elab_errors: &[ElabError],
    warnings: &[ElabError],
    max_errors: usize,
) -> bool {
    // Deduplicate errors by span to reduce noise
    let deduped_errors = deduplicate_errors(elab_errors);

    // Limit the number of errors displayed
    let limit = if max_errors == 0 {
        deduped_errors.len()
    } else {
        max_errors
    };
    let errors_to_show = &deduped_errors[..limit.min(deduped_errors.len())];
    let omitted_errors = deduped_errors.len().saturating_sub(limit);

    // Create hint tracker for dedup and category suppression
    let use_hints = hints::should_emit_hints();
    let mut hint_tracker = HintTracker::new();

    // Render elaboration errors
    for error in errors_to_show {
        let tracker = if use_hints {
            Some(&mut hint_tracker)
        } else {
            None
        };
        render_elab_error_with_source_map(
            default.source,
            default.filename,
            source_map,
            error,
            tracker,
        );
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning_with_source_map(default.source, default.filename, source_map, warning);
    }

    print_summary(
        &hint_tracker,
        deduped_errors.len(),
        warnings.len(),
        omitted_errors,
    )
}

/// Render elaboration errors, parse errors, and warnings with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_limited(
    src: &SourceRef<'_>,
    elab_errors: &[ElabError],
    parse_errors: &[ParseError],
    warnings: &[ElabError],
    max_errors: usize,
) -> bool {
    // Deduplicate parse errors by span
    let deduped_parse = deduplicate_parse_errors(parse_errors);
    let deduped_elab = deduplicate_errors(elab_errors);

    // Calculate limits
    let total_count = deduped_parse.len() + deduped_elab.len();
    let limit = if max_errors == 0 {
        total_count
    } else {
        max_errors
    };

    // Create hint tracker for dedup and category suppression
    let use_hints = hints::should_emit_hints();
    let mut hint_tracker = HintTracker::new();

    // Render parse errors first (up to limit)
    let mut rendered = 0;
    for error in &deduped_parse {
        if max_errors > 0 && rendered >= limit {
            break;
        }
        render_parse_error(src.source, src.filename, error);
        rendered += 1;
    }

    // Render elaboration errors (remaining budget)
    for error in &deduped_elab {
        if max_errors > 0 && rendered >= limit {
            break;
        }
        let tracker = if use_hints {
            Some(&mut hint_tracker)
        } else {
            None
        };
        render_elab_error(src.source, src.filename, error, tracker, None);
        rendered += 1;
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning(src.source, src.filename, warning);
    }

    let omitted = total_count.saturating_sub(limit);
    print_summary(&hint_tracker, total_count, warnings.len(), omitted)
}

/// Deduplicate errors by their span to reduce cascading error noise.
///
/// Keeps the first error encountered for each unique span.
pub(super) fn deduplicate_errors(errors: &[ElabError]) -> Vec<&ElabError> {
    use std::collections::HashSet;

    let mut seen_spans = HashSet::new();
    let mut result = Vec::new();

    for error in errors {
        let key = (
            error.span.start,
            error.span.end,
            error.file_path.as_ref().map(|p| p.to_path_buf()),
        );
        if seen_spans.insert(key) {
            result.push(error);
        }
    }

    result
}

/// Deduplicate parse errors by their span.
pub(super) fn deduplicate_parse_errors(errors: &[ParseError]) -> Vec<&ParseError> {
    use std::collections::HashSet;

    let mut seen_spans = HashSet::new();
    let mut result = Vec::new();

    for error in errors {
        let key = (error.span.start, error.span.end);
        if seen_spans.insert(key) {
            result.push(error);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────
    // format_warning_suffix
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn warning_suffix_zero() {
        assert_eq!(format_warning_suffix(0), "");
    }

    #[test]
    fn warning_suffix_one() {
        assert_eq!(format_warning_suffix(1), "; 1 warning emitted");
    }

    #[test]
    fn warning_suffix_many() {
        assert_eq!(format_warning_suffix(5), "; 5 warnings emitted");
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — errors only
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_single_error_no_warnings() {
        let (text, has_errors) = format_summary(0, 1, 0, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 1 error"));
        assert!(!text.contains("errors")); // singular
        assert!(!text.contains("warning"));
    }

    #[test]
    fn summary_multiple_errors_no_warnings() {
        let (text, has_errors) = format_summary(0, 3, 0, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 3 errors"));
    }

    #[test]
    fn summary_errors_with_warnings() {
        let (text, has_errors) = format_summary(0, 2, 3, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 2 errors"));
        assert!(text.contains("; 3 warnings emitted"));
    }

    #[test]
    fn summary_errors_with_one_warning() {
        let (text, _) = format_summary(0, 2, 1, 0);
        assert!(text.contains("; 1 warning emitted"));
        assert!(!text.contains("warnings")); // singular
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — omitted errors
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_with_omitted_errors() {
        let (text, has_errors) = format_summary(0, 5, 0, 3);
        assert!(has_errors);
        assert!(text.contains("3 not shown"));
        assert!(text.contains("--max-errors=0"));
    }

    #[test]
    fn summary_omitted_with_warnings() {
        let (text, _) = format_summary(0, 5, 2, 3);
        assert!(text.contains("3 not shown"));
        assert!(text.contains("; 2 warnings emitted"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — hint suppression
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_with_suppressed_hints_singular() {
        let (text, _) = format_summary(1, 2, 0, 0);
        assert!(text.contains("1 additional diagnostic hint suppressed"));
        assert!(text.contains("--verbose-hints"));
        assert!(!text.contains("hints suppressed")); // singular
    }

    #[test]
    fn summary_with_suppressed_hints_plural() {
        let (text, _) = format_summary(5, 2, 0, 0);
        assert!(text.contains("5 additional diagnostic hints suppressed"));
    }

    #[test]
    fn summary_suppressed_plus_omitted_plus_warnings() {
        let (text, has_errors) = format_summary(3, 10, 2, 5);
        assert!(has_errors);
        assert!(text.contains("3 additional diagnostic hints suppressed"));
        assert!(text.contains("5 not shown"));
        assert!(text.contains("; 2 warnings emitted"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — warnings only / no diagnostics
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_warnings_only() {
        let (text, has_errors) = format_summary(0, 0, 3, 0);
        assert!(!has_errors);
        assert!(text.contains("warning: 3 warnings emitted"));
    }

    #[test]
    fn summary_one_warning_only() {
        let (text, has_errors) = format_summary(0, 0, 1, 0);
        assert!(!has_errors);
        assert!(text.contains("1 warning emitted"));
        assert!(!text.contains("warnings")); // singular
    }

    #[test]
    fn summary_no_diagnostics() {
        let (text, has_errors) = format_summary(0, 0, 0, 0);
        assert!(!has_errors);
        assert!(text.is_empty());
    }
}
