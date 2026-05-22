//! Source span and line index utilities for error reporting.
//!
//! Spans track byte offsets into source text, enabling precise error messages.
//! The `LineIndex` maps byte offsets to line/column locations.

mod line_index;

pub use line_index::LineIndex;

use serde::{Deserialize, Serialize};
use std::fmt;

/// A span of source code represented as byte offsets.
///
/// Spans are half-open intervals: `[start, end)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Span {
    /// Start byte offset (inclusive)
    pub start: u32,
    /// End byte offset (exclusive)
    pub end: u32,
}

impl Span {
    /// Create a new span from start and end byte offsets.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Create an empty span at a position.
    #[must_use]
    pub const fn empty(pos: u32) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Create a span covering a single byte.
    #[must_use]
    pub const fn single(pos: u32) -> Self {
        Self {
            start: pos,
            end: pos + 1,
        }
    }

    /// Length of the span in bytes.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Check if the span is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into one covering both.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            start: if self.start < other.start {
                self.start
            } else {
                other.start
            },
            end: if self.end > other.end {
                self.end
            } else {
                other.end
            },
        }
    }

    /// Get the source text for this span.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }

    /// Check if this span contains a byte offset.
    #[must_use]
    pub const fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Extend this span to include another position.
    #[must_use]
    pub const fn extend_to(self, end: u32) -> Self {
        Self {
            start: self.start,
            end,
        }
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// Trait for AST nodes that have a source span.
pub trait Spanned {
    /// Get the span of this node.
    fn span(&self) -> Span;
}

impl Spanned for Span {
    fn span(&self) -> Span {
        *self
    }
}

/// Source location with line and column numbers (1-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed, in bytes)
    pub column: u32,
}

impl Location {
    /// Create a new location.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_basics() {
        let span = Span::new(10, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
        assert!(span.contains(15));
        assert!(!span.contains(20));
    }

    #[test]
    fn test_span_merge() {
        let a = Span::new(5, 10);
        let b = Span::new(15, 20);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(5, 20));
    }

    #[test]
    fn test_span_text() {
        let source = "hello world";
        let span = Span::new(0, 5);
        assert_eq!(span.text(source), "hello");
    }
}
