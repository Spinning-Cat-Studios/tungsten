//! Diagnostic rendering using ariadne.

pub mod hints;
mod renderers;
mod rendering;
mod structural_divergence;

pub use hints::{set_hint_mode, HintMode};
pub use rendering::{
    render_diagnostics_limited, render_diagnostics_with_source_map_limited, SourceRef,
};

use super::modules::SourceMap;
use super::output::format_type_for_display;
use crate::{ElabError, ElabErrorKind, ParseError};
use std::cell::Cell;

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

/// The structural-divergence note for an error whose two types render
/// identically (ADR 21.7.26f / D2). `None` for every other error and for
/// mismatches whose renders already differ — the enrichment adds no noise to
/// the common case.
pub(super) fn type_mismatch_divergence_note(error: &ElabError) -> Option<String> {
    match &error.kind {
        ElabErrorKind::TypeMismatch { expected, found } => {
            structural_divergence::identical_render_note(expected, found)
        }
        _ => None,
    }
}

/// Format the primary label message with proper type display.
///
/// This replaces raw type encodings with human-readable, depth-limited formatting.
pub(super) fn format_primary_label(error: &ElabError) -> String {
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
pub(super) fn format_error_message(error: &ElabError) -> String {
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
    let src = SourceRef { source, filename };
    render_diagnostics_limited(&src, elab_errors, parse_errors, &[], max_errors);
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
    let default = SourceRef {
        source: default_source,
        filename: default_filename,
    };
    render_diagnostics_with_source_map_limited(
        &default,
        source_map,
        elab_errors,
        warnings,
        max_errors,
    )
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
    let src = SourceRef { source, filename };
    render_diagnostics_limited(&src, elab_errors, parse_errors, warnings, max_errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::diagnostics::renderers::get_error_source;
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

    // ── identical-render enrichment dispatch (ADR 21.7.26f / D2) ────────────

    /// `StrMap<CtorBucket>` where one argument is the resolved ADT and the
    /// other an unresolved application — the two render identically.
    fn identically_rendering_mismatch() -> ElabError {
        let resolved = Type::App(
            "StrMap".to_string(),
            vec![Type::Adt(
                "CtorBucket".to_string(),
                vec![],
                vec![("Bucket".to_string(), Type::Nat)],
            )],
        );
        let unresolved = Type::App(
            "StrMap".to_string(),
            vec![Type::App("CtorBucket".to_string(), vec![])],
        );
        ElabError::type_mismatch(Span::new(0, 10), resolved, unresolved)
    }

    #[test]
    fn identical_render_mismatch_gets_a_divergence_note() {
        let note = type_mismatch_divergence_note(&identically_rendering_mismatch())
            .expect("an identical-render mismatch warrants a note");
        assert!(note.contains("type argument #1 of `StrMap`"), "{note}");
    }

    #[test]
    fn an_ordinary_mismatch_gets_no_note() {
        // The zero-noise guarantee at the dispatch layer.
        let error = ElabError::type_mismatch(Span::new(0, 10), Type::Nat, Type::Bool);
        assert_eq!(type_mismatch_divergence_note(&error), None);
    }

    #[test]
    fn a_non_mismatch_error_gets_no_note() {
        let error = ElabError::new(Span::new(0, 10), crate::ElabErrorKind::NoMainFunction);
        assert_eq!(type_mismatch_divergence_note(&error), None);
    }
}
