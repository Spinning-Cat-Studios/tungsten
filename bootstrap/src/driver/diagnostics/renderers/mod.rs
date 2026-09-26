//! Ariadne-based rendering of parse errors, elaboration errors, and warnings.

use super::format_error_message;
use super::format_primary_label;
use super::hints::{self, DiagnosticHint, HintTracker};
use super::type_mismatch_divergence_note;
use crate::driver::modules::SourceMap;
use crate::{ElabError, Note, ParseError};
use ariadne::{Color, Label, ReportKind, Source};
use report_factory::build_report;
use std::path::Path;

mod report_factory;

/// Join every span-less note into the single note ariadne will render.
///
/// `None` when there are none, so the caller skips `with_note` entirely rather
/// than emitting an empty block.
///
/// Multiple notes are joined one-per-line. ariadne prefixes only the *first*
/// line with `Note:` and does not indent the rest into its box, so subsequent
/// notes render flush-left — an existing artifact of any multi-line note here
/// (`format_trace_note` already produces one), not something this introduces.
/// A blank-line separator would double that gap for no gain, so the join is a
/// single newline.
pub(super) fn combine_notes(notes: &[String]) -> Option<String> {
    match notes {
        [] => None,
        [only] => Some(only.clone()),
        many => Some(many.join("\n")),
    }
}

/// Render cross-file notes as separate ariadne report blocks (ADR 15.5.26a).
///
/// Notes pointing to files available in the SourceMap get a full ariadne block;
/// notes referencing unavailable files degrade to textual `file:line` output.
fn render_cross_file_notes(notes: &[&Note], source_map: Option<&SourceMap>) {
    for note in notes {
        let (Some(note_file), Some(note_span)) = (note.file_path.as_ref(), note.span) else {
            continue;
        };
        let note_start = note_span.start as usize;
        let note_end = note_span.end as usize;

        if let Some(note_source) = source_map.and_then(|sm| sm.get(note_file)) {
            let note_filename = note_file.display().to_string();
            let note_fname: &str = &note_filename;
            build_report(
                ReportKind::Custom("note", Color::Blue),
                note_fname,
                note_start,
            )
            .with_message(&note.message)
            .with_label(
                Label::new((note_fname, note_start..note_end))
                    .with_message(&note.message)
                    .with_color(Color::Blue),
            )
            .finish()
            .eprint((note_fname, Source::from(note_source)))
            .unwrap();
        } else {
            // Source unavailable — degrade to textual note (ADR 15.5.26a §2.3)
            eprintln!(
                "  note: {} (at {}:{}..{})",
                note.message,
                note_file.display(),
                note_start,
                note_end
            );
        }
    }
}

/// Format elaboration trace frames as a combined note string (ADR 15.5.26a).
///
/// Returns `None` if the trace is empty. When `source_map` is provided,
/// byte offsets are converted to 1-based line numbers.
fn format_trace_note(
    trace: &[crate::elaborate::TraceFrame],
    source_map: Option<&SourceMap>,
) -> Option<String> {
    if trace.is_empty() {
        return None;
    }
    let mut text = String::from("type expectation flows from:");
    for frame in trace {
        let location = byte_offset_to_line(source_map, &frame.file_path, frame.span.start);
        text.push_str(&format!(
            "\n          → {}:{}  {}",
            frame.file_path.display(),
            location,
            frame.message
        ));
    }
    Some(text)
}

/// Convert a byte offset to a 1-based line number using the source map.
/// Falls back to the raw byte offset if the source is unavailable.
fn byte_offset_to_line(
    source_map: Option<&SourceMap>,
    file_path: &std::path::Path,
    offset: u32,
) -> String {
    if let Some(source) = source_map.and_then(|sm| sm.get(file_path)) {
        let line = source[..offset as usize]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        line.to_string()
    } else {
        offset.to_string()
    }
}

/// Get the appropriate source and filename for an error.
pub(super) fn get_error_source<'a>(
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

pub(super) fn render_parse_error(source: &str, filename: &str, error: &ParseError) {
    let span = error.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    let message = error.to_diagnostic().message;
    let label_text = format!("{:?}", error.kind);

    let mut report = build_report(ReportKind::Error, filename, start)
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

/// The parenthetical hint printed under an out-of-bounds-span error.
///
/// `NoMainFunction` uses a synthetic end-of-file span by design (there is no
/// better location when nothing matched), so its out-of-bounds position is NOT
/// a file_path tracking symptom. Emitting that hint here caused a documented
/// mis-diagnosis (superseded ADR 3.7.26f blamed path tracking; the real cause
/// was an empty elaborated def set — see ADR 4.7.26c). Give a cache-aware
/// breadcrumb instead, since a stale/bodyless elab-cache entry is the most
/// common way a file with functions elaborates to zero defs. All other kinds
/// keep the original file_path hint, which is accurate for a genuine span/source
/// mismatch.
fn out_of_bounds_hint(kind: &crate::ElabErrorKind) -> &'static str {
    match kind {
        crate::ElabErrorKind::NoMainFunction => {
            "(no `main` in the elaborated definitions; if this file defines \
             functions, the elaboration cache may be stale — run `tungsten cache clean`)"
        }
        _ => "(span out of bounds; this may indicate a file_path tracking issue)",
    }
}

/// Fallback renderer for errors with out-of-bounds spans.
///
/// When the span doesn't match the source (e.g., file_path tracking issue),
/// ariadne can't render — emit plain text diagnostics instead.
fn render_elab_error_fallback(filename: &str, error: &ElabError) {
    let start = error.span.start as usize;
    let end = error.span.end as usize;
    eprintln!(
        "[{}] Error: {}",
        error.kind.code(),
        format_error_message(error),
    );
    eprintln!("    at {}:{}..{}", filename, start, end);
    eprintln!("    {}", out_of_bounds_hint(&error.kind));
    if let Some(divergence) = type_mismatch_divergence_note(error) {
        for line in divergence.lines() {
            eprintln!("    note: {line}");
        }
    }
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
}

pub(super) fn render_elab_error(
    source: &str,
    filename: &str,
    error: &ElabError,
    hint_tracker: Option<&mut HintTracker>,
    source_map: Option<&SourceMap>,
) {
    let span = error.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    // Check for invalid span - this can happen if file_path wasn't set correctly
    // and the span refers to a different file than the source we have.
    let source_len = source.len();
    if start >= source_len || end > source_len || start > end {
        render_elab_error_fallback(filename, error);
        return;
    }

    let mut report = build_report(ReportKind::Error, filename, start)
        .with_code(error.kind.code())
        .with_message(format_error_message(error));

    // Primary label with a concise description
    report = report.with_label(
        Label::new((filename, start..end))
            .with_message(format_primary_label(error))
            .with_color(Color::Red),
    );

    // Add notes: those with spans become secondary labels, those without become notes.
    // Cross-file notes (with file_path differing from primary) are collected for
    // separate rendering (ADR 15.5.26a).
    let mut cross_file_notes = Vec::new();
    // ariadne's `Report::with_note` OVERWRITES rather than appends, so every
    // span-less note must be gathered and set ONCE. Before ADR 8.8.26d this
    // was a loop plus two more `with_note` calls below, and only the last one
    // survived — an error carrying notes, a divergence note and a trace showed
    // the trace alone. ADR 7.8.26e hit it and worked around it by moving facts
    // into the message; the notes were there all along, silently dropped.
    let mut plain_notes: Vec<String> = Vec::new();
    for note in &error.notes {
        if let Some(ref note_file) = note.file_path {
            // Cross-file note — render separately after the primary report
            cross_file_notes.push(note);
            continue;
        }
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
            plain_notes.push(note.message.clone());
        }
    }

    // Say where two identically-rendering types actually differ (ADR 21.7.26f)
    if let Some(divergence) = type_mismatch_divergence_note(error) {
        plain_notes.push(divergence);
    }

    // Add elaboration trace as a combined note (ADR 15.5.26a)
    if let Some(trace_text) = format_trace_note(&error.trace, source_map) {
        plain_notes.push(trace_text);
    }

    if let Some(combined) = combine_notes(&plain_notes) {
        report = report.with_note(combined);
    }

    // Add help
    if let Some(help) = &error.help {
        report = report.with_help(help);
    }

    report
        .finish()
        .eprint((filename, Source::from(source)))
        .unwrap();

    // Render cross-file notes as separate report blocks (ADR 15.5.26a)
    render_cross_file_notes(&cross_file_notes, source_map);

    // Emit diagnostic hints if enabled
    if hints::should_emit_hints() {
        // Use `filename` (already resolved to the correct file by get_error_source)
        // rather than error.file_path, which may point to a wrong workspace sibling
        // when multiple single-file programs define `fn main()` in the same directory.
        let raw_hints = hints::select_hints(&error.kind, Some(Path::new(filename)));
        let to_show = if let Some(tracker) = hint_tracker {
            tracker.filter_hints(&error.kind, raw_hints)
        } else {
            raw_hints
        };
        if !to_show.is_empty() {
            eprint!("{}", hints::format_hints(&to_show));
        }
    }
}

pub(super) fn render_warning(source: &str, filename: &str, warning: &ElabError) {
    let span = warning.span;
    let start: usize = span.start as usize;
    let end: usize = span.end as usize;

    let mut report = build_report(ReportKind::Warning, filename, start)
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

pub(super) fn render_elab_error_with_source_map(
    default_source: &str,
    default_filename: &str,
    source_map: &SourceMap,
    error: &ElabError,
    hint_tracker: Option<&mut HintTracker>,
) {
    let (source, filename) = get_error_source(
        default_source,
        default_filename,
        source_map,
        error.file_path.as_deref(),
    );
    render_elab_error(source, &filename, error, hint_tracker, Some(source_map));
}

pub(super) fn render_warning_with_source_map(
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
mod tests;
