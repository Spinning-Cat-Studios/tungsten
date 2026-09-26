//! Patterns keyed on a **failure**: something went wrong and printed text the
//! user can paste.
//!
//! The split from [`super::questions`] is the registry's own long-standing
//! distinction, promoted from prose to structure when `patterns/` reached its
//! directory-size limit. It is a real seam rather than a filing convenience:
//! these tables are reached by matching against an error message, so their
//! keywords are drawn from diagnostics the compiler emits, and the hazard is a
//! keyword drifting out of sync with the message that produces it. The
//! `questions` tables have no message to drift from and fail the opposite way —
//! by being undiscoverable.
//!
//! Order within [`super::all_patterns`] is load-bearing and lives there, not
//! here.

pub(super) use super::{ErrorPattern, ToolSuggestion};

mod build;
mod comparator;
mod elaboration;
mod resolution;
mod runtime;
mod termination;

pub(super) use build::BUILD_PATTERNS;
pub(super) use comparator::COMPARATOR_PATTERNS;
pub(super) use elaboration::ELABORATION_PATTERNS;
pub(super) use resolution::RESOLUTION_PATTERNS;
pub(super) use runtime::RUNTIME_PATTERNS;
pub(super) use termination::TERMINATION_PATTERNS;
