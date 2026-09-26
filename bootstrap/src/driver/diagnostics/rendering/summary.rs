//! The diagnostic summary footer.
//!
//! Reports **two** error figures, not one (ADR 7.8.26d §2.3): the displayed
//! count (post-deduplication, what the reader can actually see) and the raw
//! pre-deduplication count. Before this, the headline number was the count of
//! distinct *spans*, so an amplification regression was invisible in ordinary
//! output — you had to go looking for it.

use crate::driver::diagnostics::hints::HintTracker;

/// Print the error/warning summary footer.
///
/// Handles hint suppression counts, omitted error counts, and warning counts.
/// `raw_errors` is the pre-deduplication count; when it exceeds `total_errors`
/// the difference is reported rather than silently absorbed.
/// Returns `true` if there were errors.
pub(super) fn print_summary(
    hint_tracker: &HintTracker,
    total_errors: usize,
    total_warnings: usize,
    omitted: usize,
    raw_errors: usize,
) -> bool {
    let (text, has_errors) = format_summary(
        hint_tracker.suppressed_count(),
        total_errors,
        total_warnings,
        omitted,
        raw_errors,
    );
    if !text.is_empty() {
        eprint!("{}", text);
    }
    has_errors
}

/// Build the summary text. Returns `(text, has_errors)`.
///
/// Pure function extracted from `print_summary` for testability.
pub(super) fn format_summary(
    suppressed_hints: usize,
    total_errors: usize,
    total_warnings: usize,
    omitted: usize,
    raw_errors: usize,
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
        out.push_str(&format!(
            "error: aborting due to {} error{}{}{}\n",
            total_errors,
            if total_errors == 1 { "" } else { "s" },
            format_error_notes(total_errors, omitted, raw_errors),
            format_warning_suffix(total_warnings),
        ));
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

/// Format the parenthesised notes that qualify the displayed error count.
///
/// Two notes can appear, in this order: how many errors were folded away by
/// deduplication, and how many were withheld by `--max-errors`. Both are
/// omitted when they would say nothing, so a run with neither renders exactly
/// as it did before ADR 7.8.26d.
fn format_error_notes(total_errors: usize, omitted: usize, raw_errors: usize) -> String {
    let mut notes: Vec<String> = Vec::new();
    if raw_errors > total_errors {
        notes.push(format!("{} before deduplication", raw_errors));
    }
    if omitted > 0 {
        notes.push(format!(
            "{} not shown; use --max-errors=0 to see all",
            omitted
        ));
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join("; "))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `format_summary` with `raw_errors` equal to the displayed count — the
    /// shape every pre-ADR-7.8.26d test asserted.
    fn summary_no_amplification(
        suppressed_hints: usize,
        total_errors: usize,
        total_warnings: usize,
        omitted: usize,
    ) -> (String, bool) {
        format_summary(
            suppressed_hints,
            total_errors,
            total_warnings,
            omitted,
            total_errors,
        )
    }

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
    // format_error_notes
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn error_notes_empty_when_nothing_to_say() {
        assert_eq!(format_error_notes(3, 0, 3), "");
    }

    #[test]
    fn error_notes_reports_dedup_only() {
        assert_eq!(
            format_error_notes(18, 0, 527),
            " (527 before deduplication)"
        );
    }

    #[test]
    fn error_notes_reports_omitted_only() {
        assert_eq!(
            format_error_notes(5, 3, 5),
            " (3 not shown; use --max-errors=0 to see all)"
        );
    }

    #[test]
    fn error_notes_reports_both_in_order() {
        assert_eq!(
            format_error_notes(18, 3, 527),
            " (527 before deduplication; 3 not shown; use --max-errors=0 to see all)"
        );
    }

    /// A raw count *below* the displayed count cannot happen (dedup only
    /// removes), but must not render a nonsense note if it ever did.
    #[test]
    fn error_notes_ignores_raw_below_displayed() {
        assert_eq!(format_error_notes(5, 0, 2), "");
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — errors only
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_single_error_no_warnings() {
        let (text, has_errors) = summary_no_amplification(0, 1, 0, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 1 error"));
        assert!(!text.contains("errors")); // singular
        assert!(!text.contains("warning"));
    }

    #[test]
    fn summary_multiple_errors_no_warnings() {
        let (text, has_errors) = summary_no_amplification(0, 3, 0, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 3 errors"));
    }

    #[test]
    fn summary_errors_with_warnings() {
        let (text, has_errors) = summary_no_amplification(0, 2, 3, 0);
        assert!(has_errors);
        assert!(text.contains("aborting due to 2 errors"));
        assert!(text.contains("; 3 warnings emitted"));
    }

    #[test]
    fn summary_errors_with_one_warning() {
        let (text, _) = summary_no_amplification(0, 2, 1, 0);
        assert!(text.contains("; 1 warning emitted"));
        assert!(!text.contains("warnings")); // singular
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — the two-figure report (ADR 7.8.26d §2.3)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_reports_pre_dedup_count() {
        let (text, has_errors) = format_summary(0, 18, 0, 0, 527);
        assert!(has_errors);
        assert!(text.contains("aborting due to 18 errors (527 before deduplication)"));
    }

    #[test]
    fn summary_omits_pre_dedup_count_when_nothing_deduplicated() {
        let (text, _) = format_summary(0, 18, 0, 0, 18);
        assert!(text.contains("aborting due to 18 errors\n"));
        assert!(!text.contains("before deduplication"));
    }

    #[test]
    fn summary_reports_pre_dedup_count_alongside_warnings() {
        let (text, _) = format_summary(0, 2, 3, 0, 9);
        assert!(text.contains("aborting due to 2 errors (9 before deduplication)"));
        assert!(text.contains("; 3 warnings emitted"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — omitted errors
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_with_omitted_errors() {
        let (text, has_errors) = summary_no_amplification(0, 5, 0, 3);
        assert!(has_errors);
        assert!(text.contains("3 not shown"));
        assert!(text.contains("--max-errors=0"));
    }

    #[test]
    fn summary_omitted_with_warnings() {
        let (text, _) = summary_no_amplification(0, 5, 2, 3);
        assert!(text.contains("3 not shown"));
        assert!(text.contains("; 2 warnings emitted"));
    }

    #[test]
    fn summary_omitted_and_deduplicated_together() {
        let (text, _) = format_summary(0, 5, 0, 3, 40);
        assert!(text.contains("(40 before deduplication; 3 not shown"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_summary — hint suppression
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn summary_with_suppressed_hints_singular() {
        let (text, _) = summary_no_amplification(1, 2, 0, 0);
        assert!(text.contains("1 additional diagnostic hint suppressed"));
        assert!(text.contains("--verbose-hints"));
        assert!(!text.contains("hints suppressed")); // singular
    }

    #[test]
    fn summary_with_suppressed_hints_plural() {
        let (text, _) = summary_no_amplification(5, 2, 0, 0);
        assert!(text.contains("5 additional diagnostic hints suppressed"));
    }

    #[test]
    fn summary_suppressed_plus_omitted_plus_warnings() {
        let (text, has_errors) = summary_no_amplification(3, 10, 2, 5);
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
        let (text, has_errors) = summary_no_amplification(0, 0, 3, 0);
        assert!(!has_errors);
        assert!(text.contains("warning: 3 warnings emitted"));
    }

    #[test]
    fn summary_one_warning_only() {
        let (text, has_errors) = summary_no_amplification(0, 0, 1, 0);
        assert!(!has_errors);
        assert!(text.contains("1 warning emitted"));
        assert!(!text.contains("warnings")); // singular
    }

    #[test]
    fn summary_no_diagnostics() {
        let (text, has_errors) = summary_no_amplification(0, 0, 0, 0);
        assert!(!has_errors);
        assert!(text.is_empty());
    }
}
