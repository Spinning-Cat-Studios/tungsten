//! Diagnostic component types: Severity, Label, Note, Suggestion.
//!
//! Supporting types used by `Diagnostic` for structured error reporting.

use crate::span::Span;
use std::fmt;

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A fatal error that prevents compilation
    Error,
    /// A warning that doesn't prevent compilation
    Warning,
    /// Informational note
    Note,
    /// A help message with suggestions
    Help,
}

impl Severity {
    /// ANSI color code for this severity.
    #[must_use]
    pub const fn color_code(&self) -> &'static str {
        match self {
            Severity::Error => "\x1b[1;31m",   // bold red
            Severity::Warning => "\x1b[1;33m", // bold yellow
            Severity::Note => "\x1b[1;36m",    // bold cyan
            Severity::Help => "\x1b[1;32m",    // bold green
        }
    }

    /// Name of this severity level.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Help => "help",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A labeled span for diagnostic messages.
#[derive(Debug, Clone)]
pub struct Label {
    /// The span to highlight
    pub span: Span,
    /// Optional message for this label
    pub message: Option<String>,
    /// Is this the primary label?
    pub primary: bool,
}

impl Label {
    /// Create a primary label with a message.
    #[must_use]
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
            primary: true,
        }
    }

    /// Create a primary label without a message.
    #[must_use]
    pub fn primary_no_message(span: Span) -> Self {
        Self {
            span,
            message: None,
            primary: true,
        }
    }

    /// Create a secondary label with a message.
    #[must_use]
    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
            primary: false,
        }
    }
}

/// A note attached to a diagnostic.
#[derive(Debug, Clone)]
pub struct Note {
    /// The note message
    pub message: String,
}

impl Note {
    /// Create a new note.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A code suggestion for fixing an error.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Span to replace
    pub span: Span,
    /// Replacement text
    pub replacement: String,
    /// Description of the suggestion
    pub message: String,
}

impl Suggestion {
    /// Create a new suggestion.
    #[must_use]
    pub fn new(span: Span, replacement: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
            message: message.into(),
        }
    }
}
