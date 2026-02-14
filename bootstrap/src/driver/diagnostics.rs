//! Diagnostic rendering using ariadne.

use super::modules::SourceMap;
use super::output::format_type_for_display;
use crate::{ElabError, ElabErrorKind, ParseError};
use ariadne::{Color, Label, Report, ReportKind, Source};
use std::cell::Cell;
use std::path::Path;

/// Default maximum number of errors to display.
pub const DEFAULT_MAX_ERRORS: usize = 20;

// Thread-local storage for max_errors setting
thread_local! {
    static MAX_ERRORS: Cell<usize> = const { Cell::new(DEFAULT_MAX_ERRORS) };
}

/// Set the maximum number of errors to display.
///
/// This affects all subsequent calls to render_diagnostics functions.
/// Set to 0 for no limit.
pub fn set_max_errors(max: usize) {
    MAX_ERRORS.with(|m| m.set(max));
}

/// Get the current maximum number of errors setting.
pub fn get_max_errors() -> usize {
    MAX_ERRORS.with(|m| m.get())
}

/// Format the primary label message with proper type display.
///
/// This replaces raw type encodings with human-readable, depth-limited formatting.
fn format_primary_label(error: &ElabError) -> String {
    match &error.kind {
        ElabErrorKind::TypeMismatch { expected, found } => {
            format!(
                "expected `{}`, found `{}`",
                format_type_for_display(expected),
                format_type_for_display(found)
            )
        }
        ElabErrorKind::ExpectedFunction(found) => {
            format!(
                "expected function, found `{}`",
                format_type_for_display(found)
            )
        }
        ElabErrorKind::ExpectedType { expected, found } => {
            format!(
                "expected {}, found `{}`",
                expected,
                format_type_for_display(found)
            )
        }
        _ => error.primary_label_message(),
    }
}

/// Format the main error message with proper type display.
///
/// This replaces raw type encodings with human-readable, depth-limited formatting.
fn format_error_message(error: &ElabError) -> String {
    match &error.kind {
        ElabErrorKind::TypeMismatch { expected, found } => {
            format!(
                "expected `{}`, found `{}`",
                format_type_for_display(expected),
                format_type_for_display(found)
            )
        }
        ElabErrorKind::ExpectedFunction(found) => {
            format!(
                "expected function, found `{}`",
                format_type_for_display(found)
            )
        }
        ElabErrorKind::ExpectedType { expected, found } => {
            format!(
                "expected `{}`, found `{}`",
                expected,
                format_type_for_display(found)
            )
        }
        _ => error.message.clone(),
    }
}

/// Render elaboration and parse errors to stderr.
pub fn render_diagnostics(
    source: &str,
    filename: &str,
    elab_errors: &[ElabError],
    parse_errors: &[ParseError],
) {
    let max_errors = get_max_errors();
    render_diagnostics_limited(source, filename, elab_errors, parse_errors, &[], max_errors);
}

/// Render elaboration errors, parse errors, and warnings to stderr with multi-file support.
///
/// Uses the source_map to look up the correct source for each error's file_path.
/// Falls back to the default source/filename if file_path is not set or not found.
pub fn render_diagnostics_with_source_map(
    default_source: &str,
    default_filename: &str,
    source_map: &SourceMap,
    elab_errors: &[ElabError],
    warnings: &[ElabError],
) -> bool {
    let max_errors = get_max_errors();
    render_diagnostics_with_source_map_limited(
        default_source,
        default_filename,
        source_map,
        elab_errors,
        warnings,
        max_errors,
    )
}

/// Render elaboration errors with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_with_source_map_limited(
    default_source: &str,
    default_filename: &str,
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

    // Render elaboration errors
    for error in errors_to_show {
        render_elab_error_with_source_map(default_source, default_filename, source_map, error);
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning_with_source_map(default_source, default_filename, source_map, warning);
    }

    // Summary
    let total_errors = deduped_errors.len();
    let total_warnings = warnings.len();

    if total_errors > 0 {
        eprintln!();
        if omitted_errors > 0 {
            eprintln!(
                "error: aborting due to {} error{} ({} not shown; use --max-errors=0 to see all){}",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                omitted_errors,
                if total_warnings > 0 {
                    format!(
                        "; {} warning{} emitted",
                        total_warnings,
                        if total_warnings == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            );
        } else {
            eprintln!(
                "error: aborting due to {} error{}{}",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                if total_warnings > 0 {
                    format!(
                        "; {} warning{} emitted",
                        total_warnings,
                        if total_warnings == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            );
        }
        true
    } else if total_warnings > 0 {
        eprintln!();
        eprintln!(
            "warning: {} warning{} emitted",
            total_warnings,
            if total_warnings == 1 { "" } else { "s" }
        );
        false
    } else {
        false
    }
}

/// Deduplicate errors by their span to reduce cascading error noise.
///
/// Keeps the first error encountered for each unique span.
fn deduplicate_errors(errors: &[ElabError]) -> Vec<&ElabError> {
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

/// Get the appropriate source and filename for an error.
fn get_error_source<'a>(
    default_source: &'a str,
    default_filename: &str,
    source_map: &'a SourceMap,
    file_path: Option<&Path>,
) -> (&'a str, String) {
    if let Some(path) = file_path {
        if let Some(source) = source_map.get(path) {
            return (source, path.display().to_string());
        }
    }
    (default_source, default_filename.to_string())
}

/// Render elaboration errors, parse errors, and warnings to stderr.
///
/// Returns `true` if there were any errors (not just warnings).
pub fn render_diagnostics_with_warnings(
    source: &str,
    filename: &str,
    elab_errors: &[ElabError],
    parse_errors: &[ParseError],
    warnings: &[ElabError],
) -> bool {
    let max_errors = get_max_errors();
    render_diagnostics_limited(
        source,
        filename,
        elab_errors,
        parse_errors,
        warnings,
        max_errors,
    )
}

/// Render elaboration errors, parse errors, and warnings with a maximum error limit.
///
/// `max_errors` of 0 means no limit.
pub fn render_diagnostics_limited(
    source: &str,
    filename: &str,
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

    // Render parse errors first (up to limit)
    let mut rendered = 0;
    for error in &deduped_parse {
        if max_errors > 0 && rendered >= limit {
            break;
        }
        render_parse_error(source, filename, error);
        rendered += 1;
    }

    // Render elaboration errors (remaining budget)
    for error in &deduped_elab {
        if max_errors > 0 && rendered >= limit {
            break;
        }
        render_elab_error(source, filename, error);
        rendered += 1;
    }

    // Render warnings (non-fatal, not counted in max_errors)
    for warning in warnings {
        render_warning(source, filename, warning);
    }

    // Summary
    let total_errors = total_count;
    let total_warnings = warnings.len();
    let omitted = total_count.saturating_sub(limit);

    if total_errors > 0 {
        eprintln!();
        if omitted > 0 {
            eprintln!(
                "error: aborting due to {} error{} ({} not shown; use --max-errors=0 to see all){}",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                omitted,
                if total_warnings > 0 {
                    format!(
                        "; {} warning{} emitted",
                        total_warnings,
                        if total_warnings == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            );
        } else {
            eprintln!(
                "error: aborting due to {} error{}{}",
                total_errors,
                if total_errors == 1 { "" } else { "s" },
                if total_warnings > 0 {
                    format!(
                        "; {} warning{} emitted",
                        total_warnings,
                        if total_warnings == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            );
        }
        true
    } else if total_warnings > 0 {
        eprintln!();
        eprintln!(
            "warning: {} warning{} emitted",
            total_warnings,
            if total_warnings == 1 { "" } else { "s" }
        );
        false
    } else {
        false
    }
}

/// Deduplicate parse errors by their span.
fn deduplicate_parse_errors(errors: &[ParseError]) -> Vec<&ParseError> {
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

fn render_parse_error(source: &str, filename: &str, error: &ParseError) {
    let span = error.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    let message = error.to_diagnostic().message;
    let label_text = format!("{:?}", error.kind);

    let mut report = Report::build(ReportKind::Error, filename, start)
        .with_message(&message)
        .with_label(
            Label::new((filename, start..end))
                .with_message(&label_text)
                .with_color(Color::Red),
        );

    // Add suggestions as help messages
    for suggestion in &error.suggestions {
        report = report.with_help(&suggestion.message);
    }

    report
        .finish()
        .eprint((filename, Source::from(source)))
        .unwrap();
}

fn render_elab_error(source: &str, filename: &str, error: &ElabError) {
    let span = error.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    // Check for invalid span - this can happen if file_path wasn't set correctly
    // and the span refers to a different file than the source we have.
    let source_len = source.len();
    if start >= source_len || end > source_len || start > end {
        // Invalid span - can't render with ariadne, use fallback
        eprintln!(
            "[{}] Error: {}",
            error.kind.code(),
            format_error_message(error),
        );
        eprintln!("    at {}:{}..{}", filename, start, end);
        eprintln!("    (span out of bounds for source length {}; this may indicate a file_path tracking issue)", source_len);
        for note in &error.notes {
            if let Some(note_span) = note.span {
                eprintln!(
                    "    note: {} (at {}..{})",
                    note.message, note_span.start, note_span.end
                );
            } else {
                eprintln!("    note: {}", note.message);
            }
        }
        if let Some(help) = &error.help {
            eprintln!("    help: {}", help);
        }
        return;
    }

    let mut report = Report::build(ReportKind::Error, filename, start)
        .with_code(error.kind.code())
        .with_message(format_error_message(error));

    // Primary label with a concise description
    report = report.with_label(
        Label::new((filename, start..end))
            .with_message(format_primary_label(error))
            .with_color(Color::Red),
    );

    // Add notes: those with spans become secondary labels, those without become notes
    for note in &error.notes {
        if let Some(note_span) = note.span {
            // Secondary label pointing to related location
            let note_start = note_span.start as usize;
            let note_end = note_span.end as usize;
            report = report.with_label(
                Label::new((filename, note_start..note_end))
                    .with_message(&note.message)
                    .with_color(Color::Blue),
            );
        } else {
            // Plain note without location
            report = report.with_note(&note.message);
        }
    }

    // Add help
    if let Some(help) = &error.help {
        report = report.with_help(help);
    }

    report
        .finish()
        .eprint((filename, Source::from(source)))
        .unwrap();
}

fn render_warning(source: &str, filename: &str, warning: &ElabError) {
    let span = warning.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    let mut report = Report::build(ReportKind::Warning, filename, start)
        .with_code(warning.kind.code())
        .with_message(format_error_message(warning));

    // Primary label with a concise description
    report = report.with_label(
        Label::new((filename, start..end))
            .with_message(format_primary_label(warning))
            .with_color(Color::Yellow),
    );

    // Add notes: those with spans become secondary labels, those without become notes
    for note in &warning.notes {
        if let Some(note_span) = note.span {
            // Secondary label pointing to related location
            let note_start = note_span.start as usize;
            let note_end = note_span.end as usize;
            report = report.with_label(
                Label::new((filename, note_start..note_end))
                    .with_message(&note.message)
                    .with_color(Color::Blue),
            );
        } else {
            // Plain note without location
            report = report.with_note(&note.message);
        }
    }

    // Add help
    if let Some(help) = &warning.help {
        report = report.with_help(help);
    }

    report
        .finish()
        .eprint((filename, Source::from(source)))
        .unwrap();
}

fn render_elab_error_with_source_map(
    default_source: &str,
    default_filename: &str,
    source_map: &SourceMap,
    error: &ElabError,
) {
    let (source, filename) = get_error_source(
        default_source,
        default_filename,
        source_map,
        error.file_path.as_deref(),
    );
    render_elab_error(source, &filename, error);
}

fn render_warning_with_source_map(
    default_source: &str,
    default_filename: &str,
    source_map: &SourceMap,
    warning: &ElabError,
) {
    let (source, filename) = get_error_source(
        default_source,
        default_filename,
        source_map,
        warning.file_path.as_deref(),
    );
    render_warning(source, &filename, warning);
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use crate::ElabError; // Use public re-export
    use tungsten_core::Type;

    #[test]
    fn test_span_validation_detects_out_of_bounds() {
        // Test that the span validation logic correctly identifies invalid spans
        let source = "fn test() -> Nat { 0 }"; // 22 chars
        let source_len = source.len();

        // Valid span
        let valid_start = 0usize;
        let valid_end = 10usize;
        assert!(valid_start < source_len && valid_end <= source_len && valid_start <= valid_end);

        // Invalid: start >= source_len
        let invalid_start = 100usize;
        let _invalid_end = 110usize;
        assert!(invalid_start >= source_len);

        // Invalid: end > source_len
        let _bad_end_start = 0usize;
        let bad_end = 100usize;
        assert!(bad_end > source_len);

        // Invalid: start > end
        let reversed_start = 10usize;
        let reversed_end = 5usize;
        assert!(reversed_start > reversed_end);
    }

    #[test]
    fn test_get_error_source_with_file_path() {
        use std::path::{Path, PathBuf};

        let default_source = "default source";
        let default_filename = "main.tg";

        // Build a source map with a secondary file
        let mut source_map = SourceMap::new();
        source_map.insert(PathBuf::from("foo.tg"), "foo source content".to_string());

        // Without file_path, should return default
        let (src, name) = get_error_source(default_source, default_filename, &source_map, None);
        assert_eq!(src, default_source);
        assert_eq!(name, default_filename);

        // With file_path that exists in source_map
        let (src, name) = get_error_source(
            default_source,
            default_filename,
            &source_map,
            Some(Path::new("foo.tg")),
        );
        assert_eq!(src, "foo source content");
        assert_eq!(name, "foo.tg");

        // With file_path that doesn't exist in source_map, should fall back
        let (src, name) = get_error_source(
            default_source,
            default_filename,
            &source_map,
            Some(Path::new("nonexistent.tg")),
        );
        assert_eq!(src, default_source);
        assert_eq!(name, default_filename);
    }

    #[test]
    fn test_format_error_message_type_mismatch() {
        let span = Span::new(0, 10);
        let error = ElabError::type_mismatch(span, Type::Nat, Type::Bool);
        let message = format_error_message(&error);
        assert!(message.contains("Nat"));
        assert!(message.contains("Bool"));
    }
}
