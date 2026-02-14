//! Module path type for representing locations in the module hierarchy.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::ast::Path;

/// A module path representing a location in the module hierarchy.
///
/// Example: `["foo", "bar"]` represents the module `foo::bar`.
/// The root module is represented by an empty path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModulePath {
    /// Segments of the module path (e.g., ["foo", "bar"])
    pub segments: Vec<String>,
}

impl ModulePath {
    /// Create a new root module path (empty).
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Create a module path from segments.
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    /// Create a module path from a single segment.
    pub fn from_name(name: impl Into<String>) -> Self {
        Self {
            segments: vec![name.into()],
        }
    }

    /// Check if this is the root module.
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Create a child module path by appending a segment.
    pub fn child(&self, name: impl Into<String>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(name.into());
        Self { segments }
    }

    /// Convert from AST Path (for qualified lookups).
    pub fn from_ast_path(path: &Path) -> Self {
        Self {
            segments: path.segments.iter().map(|s| s.name.clone()).collect(),
        }
    }

    /// Get the parent module path, or None if this is root.
    pub fn parent(&self) -> Option<Self> {
        if self.segments.is_empty() {
            None
        } else {
            let mut segments = self.segments.clone();
            segments.pop();
            Some(Self { segments })
        }
    }

    /// Check if this path starts with (is at or under) the given prefix.
    ///
    /// For example:
    /// - `foo::bar::baz` starts with `foo::bar` ✓
    /// - `foo::bar::baz` starts with `foo` ✓
    /// - `foo::bar::baz` starts with `foo::bar::baz` ✓ (exact match)
    /// - `foo::bar` starts with `foo::bar::baz` ✗ (too short)
    /// - `foo::other` starts with `foo::bar` ✗ (different branch)
    pub fn starts_with(&self, prefix: &ModulePath) -> bool {
        if prefix.segments.len() > self.segments.len() {
            return false;
        }
        self.segments
            .iter()
            .zip(prefix.segments.iter())
            .all(|(a, b)| a == b)
    }

    /// Join another path onto this one (like `self / other`).
    ///
    /// Example: `foo::bar`.join(`baz::qux`) → `foo::bar::baz::qux`
    pub fn join(&self, other: &ModulePath) -> Self {
        let mut segments = self.segments.clone();
        segments.extend(other.segments.iter().cloned());
        Self { segments }
    }

    /// Create a path from raw segments (convenience for resolution).
    pub fn from_segments(segments: &[String]) -> Self {
        Self {
            segments: segments.to_vec(),
        }
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            write!(f, "<root>")
        } else {
            write!(f, "{}", self.segments.join("::"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_path_starts_with() {
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");
        let foo_bar = foo.child("bar");
        let foo_bar_baz = foo_bar.child("baz");
        let other = ModulePath::from_name("other");

        // Root starts with itself
        assert!(root.starts_with(&root));

        // Everything starts with root
        assert!(foo.starts_with(&root));
        assert!(foo_bar.starts_with(&root));

        // foo::bar::baz starts with foo::bar, foo, and itself
        assert!(foo_bar_baz.starts_with(&foo_bar));
        assert!(foo_bar_baz.starts_with(&foo));
        assert!(foo_bar_baz.starts_with(&foo_bar_baz));

        // foo::bar does NOT start with foo::bar::baz (too short)
        assert!(!foo_bar.starts_with(&foo_bar_baz));

        // foo does NOT start with other (different branch)
        assert!(!foo.starts_with(&other));

        // foo::bar does NOT start with other
        assert!(!foo_bar.starts_with(&other));
    }
}
