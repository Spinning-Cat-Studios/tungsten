//! Lexer error types.

use std::fmt;

use crate::span::Span;

use super::{Diagnostic, Label};

/// Error type for lexer errors.
#[derive(Debug, Clone)]
pub struct LexError {
    /// The span where the error occurred
    pub span: Span,
    /// The error kind
    pub kind: LexErrorKind,
}

/// Kinds of lexer errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Unexpected character
    UnexpectedChar(char),
    /// Unterminated string literal
    UnterminatedString,
    /// Unterminated character literal
    UnterminatedChar,
    /// Empty character literal
    EmptyCharLiteral,
    /// Invalid escape sequence
    InvalidEscape(char),
    /// Invalid hex escape sequence
    InvalidHexEscape,
    /// Invalid unicode escape sequence
    InvalidUnicodeEscape,
    /// Unterminated block comment
    UnterminatedBlockComment,
    /// Invalid number literal
    InvalidNumber(String),
    /// Reserved keyword used as identifier
    ReservedKeyword(String),
    /// Wrong comment syntax (-- instead of //)
    WrongCommentSyntax,
}

impl LexError {
    /// Create a new lexer error.
    #[must_use]
    pub fn new(span: Span, kind: LexErrorKind) -> Self {
        Self { span, kind }
    }

    /// Convert to a diagnostic.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        let (message, note) = match &self.kind {
            LexErrorKind::UnexpectedChar(c) => (
                format!("unexpected character `{}`", c.escape_default()),
                None,
            ),
            LexErrorKind::UnterminatedString => (
                "unterminated string literal".to_string(),
                Some("missing closing `\"`".to_string()),
            ),
            LexErrorKind::UnterminatedChar => (
                "unterminated character literal".to_string(),
                Some("missing closing `'`".to_string()),
            ),
            LexErrorKind::EmptyCharLiteral => (
                "empty character literal".to_string(),
                None,
            ),
            LexErrorKind::InvalidEscape(c) => (
                format!("invalid escape sequence `\\{}`", c.escape_default()),
                Some("valid escapes: \\n, \\r, \\t, \\\\, \\', \\\", \\0, \\xNN, \\u{NNNNNN}".to_string()),
            ),
            LexErrorKind::InvalidHexEscape => (
                "invalid hex escape sequence".to_string(),
                Some("hex escapes must be \\xNN where NN are two hex digits (e.g., \\x1b)".to_string()),
            ),
            LexErrorKind::InvalidUnicodeEscape => (
                "invalid unicode escape sequence".to_string(),
                Some("unicode escapes must be \\u{N} to \\u{NNNNNN} where N are hex digits".to_string()),
            ),
            LexErrorKind::UnterminatedBlockComment => (
                "unterminated block comment".to_string(),
                Some("missing closing `*/`".to_string()),
            ),
            LexErrorKind::InvalidNumber(s) => (
                format!("invalid number literal `{}`", s),
                None,
            ),
            LexErrorKind::ReservedKeyword(kw) => (
                format!("`{}` is a reserved keyword", kw),
                Some("this keyword is reserved for future use".to_string()),
            ),
            LexErrorKind::WrongCommentSyntax => (
                "incorrect comment syntax `--`".to_string(),
                Some("Tungsten uses C-style comments: `//` for line comments, `/* */` for block comments".to_string()),
            ),
        };

        let mut diag = Diagnostic::error(message)
            .with_code("E0001")
            .with_label(Label::primary_no_message(self.span));

        if let Some(n) = note {
            diag = diag.with_note(n);
        }

        diag
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            LexErrorKind::UnexpectedChar(c) => {
                write!(f, "unexpected character `{}`", c.escape_default())
            }
            LexErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            LexErrorKind::UnterminatedChar => write!(f, "unterminated character literal"),
            LexErrorKind::EmptyCharLiteral => write!(f, "empty character literal"),
            LexErrorKind::InvalidEscape(c) => {
                write!(f, "invalid escape sequence `\\{}`", c.escape_default())
            }
            LexErrorKind::InvalidHexEscape => write!(f, "invalid hex escape sequence"),
            LexErrorKind::InvalidUnicodeEscape => write!(f, "invalid unicode escape sequence"),
            LexErrorKind::UnterminatedBlockComment => write!(f, "unterminated block comment"),
            LexErrorKind::InvalidNumber(s) => write!(f, "invalid number literal `{}`", s),
            LexErrorKind::ReservedKeyword(kw) => write!(f, "`{}` is a reserved keyword", kw),
            LexErrorKind::WrongCommentSyntax => write!(f, "incorrect comment syntax `--`"),
        }
    }
}

impl std::error::Error for LexError {}
