//! Path type for qualified names.
//!
//! Represents `foo::bar::baz` style paths used in expressions, types, and patterns.

use crate::span::{Span, Spanned};
use serde::{Deserialize, Serialize};

use super::Ident;

/// A path with optional module segments: `foo::bar::baz`
///
/// Used for qualified names in expressions, types, and patterns.
/// A single-segment path is equivalent to an unqualified identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path {
    /// Path segments (e.g., ["foo", "bar", "baz"])
    pub segments: Vec<Ident>,
    /// Span covering the entire path
    pub span: Span,
}

impl Path {
    /// Create a simple single-segment path (an unqualified name).
    #[must_use]
    pub fn simple(name: Ident) -> Self {
        let span = name.span;
        Self {
            segments: vec![name],
            span,
        }
    }

    /// Check if this is a simple unqualified name (single segment).
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// Get the final segment (item name).
    ///
    /// # Panics
    /// Panics if the path has no segments (should never happen for valid paths).
    #[must_use]
    pub fn item_name(&self) -> &Ident {
        self.segments
            .last()
            .expect("path must have at least one segment")
    }

    /// Get module segments (all but the last).
    #[must_use]
    pub fn module_segments(&self) -> &[Ident] {
        if self.segments.is_empty() {
            &[]
        } else {
            &self.segments[..self.segments.len() - 1]
        }
    }
}

impl Spanned for Path {
    fn span(&self) -> Span {
        self.span
    }
}
