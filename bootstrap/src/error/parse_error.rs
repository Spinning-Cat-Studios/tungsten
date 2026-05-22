//! Parser error types.

use std::fmt;

use crate::span::Span;

use super::{Diagnostic, Label, Suggestion};

/// Error type for parser errors.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// The span where the error occurred
    pub span: Span,
    /// The error kind
    pub kind: ParseErrorKind,
    /// Expected tokens (for better error messages)
    pub expected: Vec<String>,
    /// Suggestions for fixes
    pub suggestions: Vec<Suggestion>,
}

/// Kinds of parser errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Unexpected token
    UnexpectedToken(String),
    /// Unexpected end of file
    UnexpectedEof,
    /// Expected a specific token
    Expected(String),
    /// Missing item
    MissingItem(String),
    /// Invalid pattern
    InvalidPattern,
    /// Invalid expression
    InvalidExpression,
    /// Invalid type
    InvalidType,
    /// Reserved keyword used
    ReservedKeyword(String),
}

impl ParseError {
    /// Create a new parse error.
    #[must_use]
    pub fn new(span: Span, kind: ParseErrorKind) -> Self {
        Self {
            span,
            kind,
            expected: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Add expected tokens for better error messages.
    #[must_use]
    pub fn with_expected(mut self, expected: Vec<String>) -> Self {
        self.expected = expected;
        self
    }

    /// Add a suggestion for fixing the error.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Convert to a diagnostic.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        let message = match &self.kind {
            ParseErrorKind::UnexpectedToken(tok) => {
                if self.expected.is_empty() {
                    format!("unexpected token `{}`", tok)
                } else if self.expected.len() == 1 {
                    format!("expected {}, found `{}`", self.expected[0], tok)
                } else {
                    format!(
                        "expected one of {}, found `{}`",
                        self.expected.join(", "),
                        tok
                    )
                }
            }
            ParseErrorKind::UnexpectedEof => {
                if self.expected.is_empty() {
                    "unexpected end of file".to_string()
                } else if self.expected.len() == 1 {
                    format!("expected {}, found end of file", self.expected[0])
                } else {
                    format!(
                        "expected one of {}, found end of file",
                        self.expected.join(", ")
                    )
                }
            }
            ParseErrorKind::Expected(what) => format!("expected {}", what),
            ParseErrorKind::MissingItem(what) => format!("missing {}", what),
            ParseErrorKind::InvalidPattern => "invalid pattern".to_string(),
            ParseErrorKind::InvalidExpression => "invalid expression".to_string(),
            ParseErrorKind::InvalidType => "invalid type".to_string(),
            ParseErrorKind::ReservedKeyword(kw) => {
                if !self.expected.is_empty() {
                    format!(
                        "`{}` is a reserved keyword and cannot be used as {}",
                        kw, self.expected[0]
                    )
                } else {
                    format!("`{}` is a reserved keyword", kw)
                }
            }
        };

        let mut diag = Diagnostic::error(message)
            .with_code("E0002")
            .with_label(Label::primary_no_message(self.span));

        // Add suggestions from the parse error
        for suggestion in &self.suggestions {
            diag = diag.with_suggestion(suggestion.clone());
        }

        diag
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::UnexpectedToken(tok) => write!(f, "unexpected token `{}`", tok),
            ParseErrorKind::UnexpectedEof => write!(f, "unexpected end of file"),
            ParseErrorKind::Expected(what) => write!(f, "expected {}", what),
            ParseErrorKind::MissingItem(what) => write!(f, "missing {}", what),
            ParseErrorKind::InvalidPattern => write!(f, "invalid pattern"),
            ParseErrorKind::InvalidExpression => write!(f, "invalid expression"),
            ParseErrorKind::InvalidType => write!(f, "invalid type"),
            ParseErrorKind::ReservedKeyword(kw) => write!(f, "`{}` is a reserved keyword", kw),
        }
    }
}

impl std::error::Error for ParseError {}
