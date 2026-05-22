//! Error types and Rust-style diagnostic rendering.
//!
//! Provides structured error types for lexer and parser errors,
//! with rich diagnostic output including source snippets, underlines,
//! notes, and suggestions.

mod components;
mod lex;
mod parse_error;
mod render;

pub use components::{Label, Note, Severity, Suggestion};
pub use lex::{LexError, LexErrorKind};
pub use parse_error::{ParseError, ParseErrorKind};
pub use render::DiagnosticRenderer;

use crate::span::Span;

/// A diagnostic message with rich context.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level
    pub severity: Severity,
    /// Error code (e.g., "E0001")
    pub code: Option<String>,
    /// Main message
    pub message: String,
    /// Labels pointing to source locations
    pub labels: Vec<Label>,
    /// Additional notes
    pub notes: Vec<Note>,
    /// Suggestions for fixes
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new warning diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Add an error code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Add a primary label.
    #[must_use]
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Add a note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(Note::new(note));
        self
    }

    /// Add a suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Get the primary span, if any.
    #[must_use]
    pub fn primary_span(&self) -> Option<Span> {
        self.labels.iter().find(|l| l.primary).map(|l| l.span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_error_diagnostic() {
        let err = LexError::new(Span::new(0, 1), LexErrorKind::UnexpectedChar('@'));
        let diag = err.to_diagnostic();
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Some("E0001".to_string()));
        assert!(diag.message.contains("unexpected character"));
    }

    #[test]
    fn test_parse_error_diagnostic() {
        let err = ParseError::new(
            Span::new(5, 10),
            ParseErrorKind::UnexpectedToken("@".to_string()),
        )
        .with_expected(vec!["`fn`".to_string(), "`type`".to_string()]);

        let diag = err.to_diagnostic();
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.message.contains("expected one of"));
    }

    #[test]
    fn test_diagnostic_rendering() {
        let source = "fn foo() {\n    let x = @;\n}";
        let renderer = DiagnosticRenderer::new(source, "test.tg").without_color();

        let diag = Diagnostic::error("unexpected character `@`")
            .with_code("E0001")
            .with_label(Label::primary(Span::new(23, 24), "not allowed here"));

        let output = renderer.render(&diag);
        assert!(output.contains("error[E0001]"));
        assert!(output.contains("unexpected character `@`"));
        assert!(output.contains("test.tg:2:13"));
    }

    #[test]
    fn test_diagnostic_with_suggestion() {
        let source = "let x = treu;";
        let renderer = DiagnosticRenderer::new(source, "test.tg").without_color();

        let diag = Diagnostic::error("cannot find value `treu` in this scope")
            .with_label(Label::primary_no_message(Span::new(8, 12)))
            .with_suggestion(Suggestion::new(
                Span::new(8, 12),
                "true",
                "a boolean literal with a similar name exists",
            ));

        let output = renderer.render(&diag);
        assert!(output.contains("help:"));
        assert!(output.contains("true"));
    }
}
