//! Diagnostic rendering in Rust-style format.
//!
//! Renders diagnostics with source snippets, underlines, notes,
//! and suggestions.

use std::io::{self, Write};

use crate::span::LineIndex;

use super::{Diagnostic, Label, Severity, Suggestion};

/// Context for rendering a single source line.
struct SourceLine<'a> {
    num: u32,
    content: &'a str,
    gutter_width: usize,
}

/// ANSI color codes for diagnostic rendering.
struct Colors {
    reset: &'static str,
    bold: &'static str,
    blue: &'static str,
    red: &'static str,
    cyan: &'static str,
}

impl Colors {
    fn new(use_color: bool) -> Self {
        if use_color {
            Self {
                reset: "\x1b[0m",
                bold: "\x1b[1m",
                blue: "\x1b[1;34m",
                red: "\x1b[1;31m",
                cyan: "\x1b[1;36m",
            }
        } else {
            Self {
                reset: "",
                bold: "",
                blue: "",
                red: "",
                cyan: "",
            }
        }
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
        let c = Colors::new(self.use_color);
        let color = if self.use_color {
            diagnostic.severity.color_code()
        } else {
            ""
        };

        // Header line: error[E0001]: message
        write!(w, "{color}{}{reset}", diagnostic.severity, reset = c.reset)?;
        if let Some(code) = &diagnostic.code {
            write!(w, "{color}[{code}]{reset}", reset = c.reset)?;
        }
        writeln!(
            w,
            "{bold}: {}{reset}",
            diagnostic.message,
            bold = c.bold,
            reset = c.reset
        )?;

        // Location line
        if let Some(primary_label) = diagnostic.labels.iter().find(|l| l.primary) {
            let loc = self.line_index.location(primary_label.span.start);
            writeln!(
                w,
                "  {blue}-->{reset} {}:{}:{}",
                self.filename,
                loc.line,
                loc.column,
                blue = c.blue,
                reset = c.reset,
            )?;
            self.render_snippet(w, &diagnostic.labels, &c)?;
        }

        // Notes
        for note in &diagnostic.notes {
            writeln!(
                w,
                "  {blue}={reset} {bold}note{reset}: {}",
                note.message,
                blue = c.blue,
                reset = c.reset,
                bold = c.bold
            )?;
        }

        // Suggestions
        for suggestion in &diagnostic.suggestions {
            self.render_suggestion(w, suggestion)?;
        }

        Ok(())
    }

    fn render_snippet<W: Write>(&self, w: &mut W, labels: &[Label], c: &Colors) -> io::Result<()> {
        // Find the range of lines to display
        let (min_line, max_line) = match self.label_line_range(labels) {
            Some(range) => range,
            None => return Ok(()),
        };

        let line_width = max_line.to_string().len();

        // Empty line before snippet
        writeln!(
            w,
            "{blue}{:>width$} |{reset}",
            "",
            width = line_width,
            blue = c.blue,
            reset = c.reset
        )?;

        // Render each line
        for line_num in min_line..=max_line {
            let line_content = self
                .line_index
                .line_content(self.source, (line_num - 1) as usize)
                .unwrap_or("");

            writeln!(
                w,
                "{blue}{:>width$} |{reset} {}",
                line_num,
                line_content,
                width = line_width,
                blue = c.blue,
                reset = c.reset,
            )?;

            let line = SourceLine {
                num: line_num,
                content: line_content,
                gutter_width: line_width,
            };
            self.render_underline_for_line(w, labels, &line, c)?;
        }

        Ok(())
    }

    /// Render the underline and label message for a single source line.
    fn render_underline_for_line<W: Write>(
        &self,
        w: &mut W,
        labels: &[Label],
        line: &SourceLine<'_>,
        c: &Colors,
    ) -> io::Result<()> {
        let (underline, has_underline) =
            self.build_underline_for_line(labels, line.num, line.content);

        if !has_underline {
            return Ok(());
        }

        let color = if labels.iter().any(|l| l.primary) {
            c.red
        } else {
            c.cyan
        };
        write!(
            w,
            "{blue}{:>width$} |{reset} {color}{}",
            "",
            underline,
            width = line.gutter_width,
            blue = c.blue,
            reset = c.reset,
        )?;

        // Add message from primary label on this line
        for label in labels {
            let start_loc = self.line_index.location(label.span.start);
            if label.primary && start_loc.line == line.num {
                if let Some(msg) = &label.message {
                    write!(w, " {}", msg)?;
                }
            }
        }
        writeln!(w, "{reset}", reset = c.reset)
    }

    /// Render a single code suggestion.
    fn render_suggestion<W: Write>(&self, w: &mut W, suggestion: &Suggestion) -> io::Result<()> {
        let reset = if self.use_color { "\x1b[0m" } else { "" };
        let blue = if self.use_color { "\x1b[1;34m" } else { "" };
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

        Ok(())
    }

    /// Find the min and max line numbers spanned by labels.
    fn label_line_range(&self, labels: &[Label]) -> Option<(u32, u32)> {
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
            None
        } else {
            Some((min_line, max_line))
        }
    }

    /// Build the underline string for a single source line.
    fn build_underline_for_line(
        &self,
        labels: &[Label],
        line_num: u32,
        line_content: &str,
    ) -> (String, bool) {
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

                while underline.len() < end_col {
                    underline.push(' ');
                }

                let marker = if label.primary { '^' } else { '-' };
                for i in start_col..end_col {
                    if i < underline.len() {
                        underline.replace_range(i..=i, &marker.to_string());
                    }
                }
            }
        }

        (underline, has_underline)
    }
}
