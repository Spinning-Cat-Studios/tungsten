//! Diagnostic deduplication.
//!
//! The key carries the **error code** as well as the span (ADR 7.8.26d D4).
//! Keying on span alone made deduplication a lossy summariser rather than a
//! noise filter: an `UndefinedVariable` and a `TypeMismatch` at one span
//! collapsed into a single displayed diagnostic, and a degraded span — the
//! module origin rather than the failing expression — makes that sharing the
//! common case rather than the rare one. Two errors at one span are the same
//! *cascade* only if they are the same *kind*.

use crate::{ElabError, ParseError};

/// What makes two elaboration errors the same diagnostic.
///
/// A named struct rather than a tuple: four positional fields would leave
/// `key.2` at every reader's mercy, and the *code* field is the one this ADR
/// added — it should be spelled, not counted to.
#[derive(PartialEq, Eq, Hash)]
struct ErrorKey {
    start: u32,
    end: u32,
    file: Option<std::path::PathBuf>,
    code: &'static str,
}

/// The deduplication key for an elaboration error.
///
/// Extracted as a named function because it is the thing worth asserting on:
/// a pure map from an error to its identity.
fn error_key(error: &ElabError) -> ErrorKey {
    ErrorKey {
        start: error.span.start,
        end: error.span.end,
        file: error.file_path.as_ref().map(|p| p.to_path_buf()),
        code: error.kind.code(),
    }
}

/// Deduplicate errors by `(span, file, code)` to reduce cascading error noise.
///
/// Keeps the first error encountered for each unique key.
pub(in crate::driver::diagnostics) fn deduplicate_errors(errors: &[ElabError]) -> Vec<&ElabError> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for error in errors {
        if seen.insert(error_key(error)) {
            result.push(error);
        }
    }

    result
}

/// Deduplicate parse errors by their span.
pub(in crate::driver::diagnostics) fn deduplicate_parse_errors(
    errors: &[ParseError],
) -> Vec<&ParseError> {
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
    use crate::elaborate::ElabErrorKind;
    use crate::span::Span;
    use std::path::PathBuf;

    fn at(start: u32, end: u32, kind: ElabErrorKind) -> ElabError {
        ElabError::new(Span::new(start, end), kind)
    }

    fn undefined_variable(start: u32, end: u32) -> ElabError {
        at(
            start,
            end,
            ElabErrorKind::UndefinedVariable("x".to_string()),
        )
    }

    fn undefined_type(start: u32, end: u32) -> ElabError {
        at(start, end, ElabErrorKind::UndefinedType("T".to_string()))
    }

    fn parse_error(start: u32, end: u32, token: &str) -> ParseError {
        ParseError::new(
            Span::new(start, end),
            crate::error::ParseErrorKind::UnexpectedToken(token.to_string()),
        )
    }

    #[test]
    fn identical_errors_collapse() {
        let errors = vec![undefined_variable(0, 5), undefined_variable(0, 5)];
        assert_eq!(deduplicate_errors(&errors).len(), 1);
    }

    /// The ADR's whole point: two *distinct root causes* sharing a degraded
    /// span are two diagnostics, not one.
    #[test]
    fn distinct_codes_at_one_span_both_survive() {
        let errors = vec![undefined_variable(0, 5), undefined_type(0, 5)];
        let deduped = deduplicate_errors(&errors);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].kind.code(), "E0001");
        assert_eq!(deduped[1].kind.code(), "E0002");
    }

    #[test]
    fn distinct_spans_at_one_code_both_survive() {
        let errors = vec![undefined_variable(0, 5), undefined_variable(7, 9)];
        assert_eq!(deduplicate_errors(&errors).len(), 2);
    }

    #[test]
    fn the_first_error_at_a_key_is_the_one_kept() {
        let mut first = undefined_variable(0, 5);
        first.help = Some("keep me".to_string());
        let errors = vec![first, undefined_variable(0, 5)];
        let deduped = deduplicate_errors(&errors);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].help.as_deref(), Some("keep me"));
    }

    #[test]
    fn the_same_span_in_different_files_does_not_collapse() {
        let a = undefined_variable(0, 5).with_file_path(PathBuf::from("a.tg"));
        let b = undefined_variable(0, 5).with_file_path(PathBuf::from("b.tg"));
        assert_eq!(deduplicate_errors(&[a, b]).len(), 2);
    }

    #[test]
    fn a_pathless_error_does_not_collapse_into_a_pathful_one() {
        let a = undefined_variable(0, 5);
        let b = undefined_variable(0, 5).with_file_path(PathBuf::from("a.tg"));
        assert_eq!(deduplicate_errors(&[a, b]).len(), 2);
    }

    #[test]
    fn no_errors_deduplicate_to_none() {
        assert!(deduplicate_errors(&[]).is_empty());
    }

    #[test]
    fn error_key_carries_the_code() {
        assert_eq!(error_key(&undefined_variable(1, 2)).code, "E0001");
        assert_eq!(error_key(&undefined_type(1, 2)).code, "E0002");
    }

    #[test]
    fn error_key_carries_both_span_endpoints() {
        let key = error_key(&undefined_variable(3, 11));
        assert_eq!((key.start, key.end), (3, 11));
    }

    #[test]
    fn error_key_carries_the_file() {
        let with_file = undefined_variable(1, 2).with_file_path(PathBuf::from("a.tg"));
        assert_eq!(error_key(&with_file).file, Some(PathBuf::from("a.tg")));
        assert_eq!(error_key(&undefined_variable(1, 2)).file, None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // parse errors — span-keyed, deliberately untouched (§3 Non-Goals)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_errors_deduplicate_by_span() {
        let errors = vec![
            parse_error(0, 5, "let"),
            parse_error(0, 5, "fn"),
            parse_error(6, 8, "let"),
        ];
        assert_eq!(deduplicate_parse_errors(&errors).len(), 2);
    }

    #[test]
    fn no_parse_errors_deduplicate_to_none() {
        assert!(deduplicate_parse_errors(&[]).is_empty());
    }
}
