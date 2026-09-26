//! Static pattern registry mapping error keywords to diagnostic tool suggestions.
//!
//! Each `ErrorPattern` maps a set of keywords to ranked tool suggestions.
//! The matching engine scores patterns by keyword overlap and deduplicates
//! suggestions across multiple matching patterns.
//!
//! Split first by **what reached the user**, then by domain. [`failures`] holds
//! the tables keyed on an error message — `runtime` for the compiled PROGRAM's
//! failures (crashes, hangs, miscompiles), `build` for the BUILD's (linking,
//! symbol resolution, self-host divergence, the compiler wedging), `elaboration`
//! for type-system patterns, `resolution` for how a name fails to resolve, `termination` for the admission gates, and
//! `comparator` for structural-comparison failures. [`questions`] holds the ones
//! with no error text at all: `profiling` for correct-but-slow and `inspection`
//! for correct-but-opaque. That two-level shape replaced a flat directory at its
//! size limit; the distinction was already the registry's, written out in prose
//! here and in the two tables' own headers.
//!
//! `all_patterns()` preserves the historical registry order (runtime's segfault
//! entry first) — equal-score matches keep insertion order, so the `questions`
//! tables go LAST: a description mentioning both a crash and a slow build, or
//! both a crash and a term dump, should still rank the crash tools first. That
//! ordering is what a reader sees, more directly than the `relevance` figures
//! are — see the score-cap note in the parent module.

// Re-exported for the submodule pattern tables.
pub(super) use super::{ErrorPattern, ToolSuggestion};

mod failures;
mod questions;

/// All patterns, in registry order.
pub(super) fn all_patterns() -> impl Iterator<Item = &'static ErrorPattern> {
    failures::COMPARATOR_PATTERNS
        .iter()
        .chain(failures::RUNTIME_PATTERNS.iter())
        .chain(failures::BUILD_PATTERNS.iter())
        .chain(failures::TERMINATION_PATTERNS.iter())
        .chain(failures::ELABORATION_PATTERNS.iter())
        .chain(failures::RESOLUTION_PATTERNS.iter())
        .chain(questions::PROFILING_PATTERNS.iter())
        .chain(questions::INSPECTION_PATTERNS.iter())
}
