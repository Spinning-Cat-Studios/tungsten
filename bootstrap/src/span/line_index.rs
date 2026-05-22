//! Line index for mapping byte offsets to line/column locations.

use super::Location;

/// Line information for a source file.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of each line start
    line_starts: Vec<u32>,
}

impl LineIndex {
    /// Build a line index from source text.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self { line_starts }
    }

    /// Convert a byte offset to a line/column location.
    #[must_use]
    pub fn location(&self, offset: u32) -> Location {
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        Location {
            line: (line + 1) as u32,
            column: (offset - line_start) + 1,
        }
    }

    /// Get the byte offset of the start of a line (0-indexed).
    #[must_use]
    pub fn line_start(&self, line: usize) -> Option<u32> {
        self.line_starts.get(line).copied()
    }

    /// Get the line content for a given line number (0-indexed).
    #[must_use]
    pub fn line_content<'a>(&self, source: &'a str, line: usize) -> Option<&'a str> {
        let start = *self.line_starts.get(line)? as usize;
        let end = self
            .line_starts
            .get(line + 1)
            .map_or(source.len(), |&e| (e as usize).saturating_sub(1));
        Some(&source[start..end])
    }

    /// Number of lines in the source.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Location;

    #[test]
    fn test_line_index() {
        let source = "line1\nline2\nline3";
        let index = LineIndex::new(source);

        assert_eq!(index.line_count(), 3);
        assert_eq!(index.location(0), Location::new(1, 1));
        assert_eq!(index.location(5), Location::new(1, 6)); // newline
        assert_eq!(index.location(6), Location::new(2, 1)); // start of line2
        assert_eq!(index.location(12), Location::new(3, 1)); // start of line3
    }

    #[test]
    fn test_line_content() {
        let source = "fn foo() {\n    bar\n}";
        let index = LineIndex::new(source);

        assert_eq!(index.line_content(source, 0), Some("fn foo() {"));
        assert_eq!(index.line_content(source, 1), Some("    bar"));
        assert_eq!(index.line_content(source, 2), Some("}"));
    }
}
