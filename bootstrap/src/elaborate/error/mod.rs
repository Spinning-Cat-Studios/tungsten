//! Error types for the elaborator.
//!
//! Following Rust's error message quality standard with spans, notes, and help.

mod constructors;
mod constructors_modules;
mod context;
mod display;
mod kind;
mod messages;

pub use constructors_modules::DuplicateImportInfo;
pub use context::*;
pub use kind::*;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use crate::span::Span;
use tungsten_core::Type;

/// An elaboration error with source location and diagnostic information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElabError {
    /// The primary error message
    pub message: String,
    /// Source span where the error occurred
    pub span: Span,
    /// The kind of error (for categorization)
    pub kind: ElabErrorKind,
    /// Additional notes explaining the error
    pub notes: Vec<Note>,
    /// Optional help text with suggestions
    pub help: Option<String>,
    /// Context explaining why we expected this type (for type errors)
    pub context: Option<ExpectedContext>,
    /// Optional file path where the error occurred (for multi-file diagnostics)
    #[serde(default)]
    pub file_path: Option<PathBuf>,
    /// Best-effort provenance trace for cross-module type flows (ADR 15.5.26a).
    /// Empty by default; populated only when the elaborator has reliable causal
    /// information (e.g., a cross-module call site with a known return type).
    #[serde(default)]
    pub trace: Vec<TraceFrame>,
}

/// A note attached to an error (secondary location or explanation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// The note message
    pub message: String,
    /// Optional span for this note
    pub span: Option<Span>,
    /// Optional file path for cross-file notes (ADR 15.5.26a).
    /// When set and different from the primary error's file, the renderer
    /// emits a separate report block for this file.
    #[serde(default)]
    pub file_path: Option<PathBuf>,
}

/// A frame in the elaboration provenance trace (ADR 15.5.26a).
///
/// Trace frames describe the causal flow of type expectations across module
/// boundaries. They are distinct from notes (local diagnostic annotations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFrame {
    /// Description of this step in the type flow
    pub message: String,
    /// Source span at this step
    pub span: Span,
    /// File where this step occurs
    pub file_path: PathBuf,
}

impl ElabError {
    /// Create a new elaboration error.
    pub fn new(span: Span, kind: ElabErrorKind) -> Self {
        let message = kind.default_message();
        Self {
            message,
            span,
            kind,
            notes: Vec::new(),
            help: None,
            context: None,
            file_path: None,
            trace: Vec::new(),
        }
    }

    /// Create an error with a custom message.
    pub fn with_message(span: Span, kind: ElabErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            kind,
            notes: Vec::new(),
            help: None,
            context: None,
            file_path: None,
            trace: Vec::new(),
        }
    }

    /// Add context explaining why a type was expected.
    #[must_use]
    pub fn with_context(mut self, context: ExpectedContext) -> Self {
        // Convert context to a span note for rendering
        self.notes.push(Note {
            message: context.explanation(),
            span: Some(context.span),
            file_path: context.file_path.clone(),
        });
        self.context = Some(context);
        self
    }

    /// Add a note to this error.
    #[must_use]
    pub fn with_note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span: None,
            file_path: None,
        });
        self
    }

    /// Add a note with a span to this error.
    #[must_use]
    pub fn with_span_note(mut self, span: Span, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span: Some(span),
            file_path: None,
        });
        self
    }

    /// Add a note with a span and file path for cross-file diagnostics.
    #[must_use]
    pub fn with_cross_file_note(
        mut self,
        span: Span,
        file_path: PathBuf,
        message: impl Into<String>,
    ) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span: Some(span),
            file_path: Some(file_path),
        });
        self
    }

    /// Add a trace frame for cross-module type flow provenance.
    #[must_use]
    pub fn with_trace_frame(
        mut self,
        span: Span,
        file_path: PathBuf,
        message: impl Into<String>,
    ) -> Self {
        self.trace.push(TraceFrame {
            message: message.into(),
            span,
            file_path,
        });
        self
    }

    /// Add help text to this error.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Set the file path where this error occurred.
    #[must_use]
    pub fn with_file_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }
}

impl fmt::Display for ElabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error[{}]: {} (at {}..{})",
            self.kind.code(),
            self.message,
            self.span.start,
            self.span.end
        )?;

        for note in &self.notes {
            if let Some(span) = note.span {
                write!(
                    f,
                    "\n  note: {} (at {}..{})",
                    note.message, span.start, span.end
                )?;
            } else {
                write!(f, "\n  note: {}", note.message)?;
            }
        }

        if let Some(ref help) = self.help {
            write!(f, "\n  help: {}", help)?;
        }

        Ok(())
    }
}

impl std::error::Error for ElabError {}

#[cfg(test)]
mod tests;
