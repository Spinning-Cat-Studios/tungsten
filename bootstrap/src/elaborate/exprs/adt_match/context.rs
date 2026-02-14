//! Context types for ADT match elaboration.

use std::collections::HashMap;

use crate::ast;

use crate::elaborate::env as elab_env;

/// Resolved context for an ADT match expression.
///
/// Contains all the information needed to elaborate the match after
/// resolving the ADT type from constructor patterns.
pub(crate) struct AdtMatchContext {
    /// The type definition for the ADT being matched
    pub type_def: elab_env::TypeDef,
    /// The constructors of the ADT
    pub constructors: Vec<elab_env::Constructor>,
    /// Whether this is a recursive (μ-type) ADT
    pub is_recursive: bool,
}

/// Classified match arms ready for elaboration.
///
/// Arms are grouped by constructor index for efficient lookup,
/// with an optional catch-all pattern for wildcard/variable arms.
///
/// IMPORTANT: When multiple arms match the same outer constructor with different
/// inner patterns (e.g., `Some(A) => ..., Some(B) => ...`), they are all stored
/// in the Vec for that constructor index. The codegen must build nested matches.
pub(crate) struct ClassifiedArms<'a> {
    /// Map from constructor index to arms matching that constructor.
    /// Multiple arms may match the same constructor with different inner patterns.
    pub ctor_arms: HashMap<usize, Vec<&'a ast::MatchArm>>,
    /// Optional catch-all arm (wildcard `_` or variable binding)
    pub catch_all: Option<&'a ast::MatchArm>,
}
