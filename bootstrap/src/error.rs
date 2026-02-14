//! Error types and Rust-style diagnostic rendering.
//!
//! Provides structured error types for lexer and parser errors,
//! with rich diagnostic output including source snippets, underlines,
//! notes, and suggestions.

use crate::span::{LineIndex, Span};
use std::fmt;
use std::io::{self, Write};

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

/// Renders diagnostics in Rust-style format.
pub struct DiagnosticRenderer<'a> {
    source: &'a str,
    filename: &'a str,
    line_index: LineIndex,
    use_color: bool,
}

impl<'a> DiagnosticRenderer<'a> {
    /// Create a new renderer.
    #[must_use]
    pub fn new(source: &'a str, filename: &'a str) -> Self {
        Self {
            source,
            filename,
            line_index: LineIndex::new(source),
            use_color: true,
        }
    }

    /// Disable color output.
    #[must_use]
    pub fn without_color(mut self) -> Self {
        self.use_color = false;
        self
    }

    /// Render a diagnostic to a string.
    #[must_use]
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let mut output = Vec::new();
        self.render_to(&mut output, diagnostic).unwrap();
        String::from_utf8(output).unwrap()
    }

    /// Render a diagnostic to a writer.
    pub fn render_to<W: Write>(&self, w: &mut W, diagnostic: &Diagnostic) -> io::Result<()> {
        let reset = if self.use_color { "\x1b[0m" } else { "" };
        let bold = if self.use_color { "\x1b[1m" } else { "" };
        let color = if self.use_color {
            diagnostic.severity.color_code()
        } else {
            ""
        };
        let blue = if self.use_color { "\x1b[1;34m" } else { "" };

        // Header line: error[E0001]: message
        write!(w, "{color}{}{reset}", diagnostic.severity)?;
        if let Some(code) = &diagnostic.code {
            write!(w, "{color}[{code}]{reset}")?;
        }
        writeln!(w, "{bold}: {}{reset}", diagnostic.message)?;

        // Location line
        if let Some(primary_label) = diagnostic.labels.iter().find(|l| l.primary) {
            let loc = self.line_index.location(primary_label.span.start);
            writeln!(
                w,
                "  {blue}-->{reset} {}:{}:{}",
                self.filename, loc.line, loc.column
            )?;

            // Source snippet with underlines
            self.render_snippet(w, &diagnostic.labels)?;
        }

        // Notes
        for note in &diagnostic.notes {
            writeln!(w, "  {blue}={reset} {bold}note{reset}: {}", note.message)?;
        }

        // Suggestions
        for suggestion in &diagnostic.suggestions {
            let help_color = if self.use_color {
                Severity::Help.color_code()
            } else {
                ""
            };
            writeln!(w, "  {help_color}help{reset}: {}", suggestion.message)?;
            let loc = self.line_index.location(suggestion.span.start);
            writeln!(
                w,
                "  {blue}-->{reset} {}:{}:{}",
                self.filename, loc.line, loc.column
            )?;
            // Show the suggested replacement
            let line_num = loc.line;
            let line_width = line_num.to_string().len();
            writeln!(w, "{blue}{:>width$} |{reset}", "", width = line_width)?;
            if let Some(line_content) = self
                .line_index
                .line_content(self.source, (line_num - 1) as usize)
            {
                let line_start = self
                    .line_index
                    .line_start((line_num - 1) as usize)
                    .unwrap_or(0);
                let col_start = (suggestion.span.start - line_start) as usize;
                let col_end = (suggestion.span.end - line_start) as usize;
                let col_end = col_end.min(line_content.len());

                // Show original line with replacement
                let before = &line_content[..col_start];
                let after = if col_end <= line_content.len() {
                    &line_content[col_end..]
                } else {
                    ""
                };
                writeln!(
                    w,
                    "{blue}{:>width$} |{reset} {}{help_color}{}{reset}{}",
                    line_num,
                    before,
                    suggestion.replacement,
                    after,
                    width = line_width
                )?;
            }
        }

        Ok(())
    }

    fn render_snippet<W: Write>(&self, w: &mut W, labels: &[Label]) -> io::Result<()> {
        let reset = if self.use_color { "\x1b[0m" } else { "" };
        let blue = if self.use_color { "\x1b[1;34m" } else { "" };
        let red = if self.use_color { "\x1b[1;31m" } else { "" };
        let cyan = if self.use_color { "\x1b[1;36m" } else { "" };

        // Find the range of lines to display
        let mut min_line = u32::MAX;
        let mut max_line = 0u32;
        for label in labels {
            let start_loc = self.line_index.location(label.span.start);
            let end_loc = self
                .line_index
                .location(label.span.end.saturating_sub(1).max(label.span.start));
            min_line = min_line.min(start_loc.line);
            max_line = max_line.max(end_loc.line);
        }

        if min_line == u32::MAX {
            return Ok(());
        }

        let line_width = max_line.to_string().len();

        // Empty line before snippet
        writeln!(w, "{blue}{:>width$} |{reset}", "", width = line_width)?;

        // Render each line
        for line_num in min_line..=max_line {
            let line_idx = (line_num - 1) as usize;
            let line_content = self
                .line_index
                .line_content(self.source, line_idx)
                .unwrap_or("");
            let _line_start = self.line_index.line_start(line_idx).unwrap_or(0);

            // Line content
            writeln!(
                w,
                "{blue}{:>width$} |{reset} {}",
                line_num,
                line_content,
                width = line_width
            )?;

            // Underlines for labels on this line
            let mut underline = String::new();
            let mut has_underline = false;

            for label in labels {
                let start_loc = self.line_index.location(label.span.start);
                let end_loc = self
                    .line_index
                    .location(label.span.end.saturating_sub(1).max(label.span.start));

                if start_loc.line <= line_num && end_loc.line >= line_num {
                    has_underline = true;
                    let start_col = if start_loc.line == line_num {
                        (start_loc.column - 1) as usize
                    } else {
                        0
                    };
                    let end_col = if end_loc.line == line_num {
                        (end_loc.column) as usize
                    } else {
                        line_content.len()
                    };

                    // Ensure underline string is long enough
                    while underline.len() < end_col {
                        underline.push(' ');
                    }

                    // Add underline characters
                    let marker = if label.primary { '^' } else { '-' };
                    for i in start_col..end_col {
                        if i < underline.len() {
                            underline.replace_range(i..i + 1, &marker.to_string());
                        }
                    }
                }
            }

            if has_underline {
                let color = if labels.iter().any(|l| l.primary) {
                    red
                } else {
                    cyan
                };
                write!(
                    w,
                    "{blue}{:>width$} |{reset} {color}{}",
                    "",
                    underline,
                    width = line_width
                )?;

                // Add message from primary label on this line
                for label in labels {
                    let start_loc = self.line_index.location(label.span.start);
                    if label.primary && start_loc.line == line_num {
                        if let Some(msg) = &label.message {
                            write!(w, " {}", msg)?;
                        }
                    }
                }
                writeln!(w, "{reset}")?;
            }
        }

        Ok(())
    }
}

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
