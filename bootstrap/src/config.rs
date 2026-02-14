//! Configuration constants for the Tungsten compiler.
//!
//! These constants control various compiler behaviors such as error message
//! formatting and suggestion thresholds. When Tungsten becomes self-hosted,
//! these may become configurable via compiler flags or a config file.

/// Maximum depth of "expected because..." context chain to display in error messages.
///
/// Higher values provide more context but may be noisy.
/// Lower values are cleaner but may hide useful information.
pub const MAX_CONTEXT_DEPTH: usize = 3;

/// Maximum Levenshtein edit distance for "did you mean" suggestions.
///
/// Names with edit distance greater than this threshold won't be suggested.
pub const SUGGESTION_MAX_DISTANCE: usize = 3;

/// Maximum ratio of edit distance to name length for suggestions.
///
/// Prevents suggesting very different names for short identifiers.
/// For example, with 0.5, we won't suggest "foo" for "x" (distance 3, ratio 3.0).
pub const SUGGESTION_MAX_RATIO: f64 = 0.5;

/// Maximum nesting depth for constructor patterns.
///
/// Level 1: `Some(x)` — constructor with variable
/// Level 2: `Some(Some(x))` — one level of nesting  
/// Level 3: `Some(Some(Some(x)))` — two levels of nesting (max)
///
/// Patterns deeper than this limit will be rejected with E0103.
pub const MAX_PATTERN_DEPTH: usize = 3;

/// Maximum nesting depth for type display in error messages.
///
/// Controls how deeply nested types are expanded before being truncated
/// with "..." in error messages. This keeps error messages readable when
/// dealing with complex recursive or deeply nested types.
///
/// Examples at depth 3:
/// - `Option<List<Nat>>` displays fully
/// - `Option<List<Pair<Nat, Bool>>>` displays fully  
/// - Deeper structures truncate: `Option<List<Pair<Foo<...>, Bar<...>>>>`
pub const MAX_TYPE_DISPLAY_DEPTH: usize = 3;
