//! Patterns keyed on a **question**, not an error: the code is correct and the
//! user still cannot see what they need.
//!
//! Both tables here exist because a miss was measured rather than reported.
//! `profiling` covers correct-but-slow (ADR 5.8.26b) and `inspection`
//! correct-but-opaque — "show me the Core term" — which had no route at all
//! until ADR 29.6.26e's retrospective went looking. That is the failure mode
//! this half of the registry has: there is no error text to paste, so nobody
//! files a bug when the answer is missing; they just do it by hand.
//!
//! Both go LAST in [`super::all_patterns`], and that ordering is deliberate —
//! see the note there.

pub(super) use super::{ErrorPattern, ToolSuggestion};

mod inspection;
mod profiling;

pub(super) use inspection::INSPECTION_PATTERNS;
pub(super) use profiling::PROFILING_PATTERNS;
