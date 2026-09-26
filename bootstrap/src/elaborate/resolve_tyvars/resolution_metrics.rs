//! Opt-in instrumentation for **deferred type-reference resolution attempts**
//! (ADR 23.7.26a §6.1 follow-up).
//!
//! `resolve_deferred_type_references` (the pass that resolves `@`-deferred
//! cross-references once every type is elaborated) makes one *resolution
//! attempt* per deferred `TyVar`/`App` reference it walks — including the
//! no-op attempts where the target turns out to be a record/stub and the
//! reference is returned unchanged. The *results* of that pass are byte-stable
//! (ADR 22.7.26d), but the *number of attempts* was observed to vary across
//! processes (§6.1: ~1,388 repeated no-op re-resolutions of one `@TypeDef`
//! reference in one run, 1 in the next). That attempt-count nondeterminism is
//! invisible to `encoding-determinism`, which only compares stored results.
//!
//! This module exposes a thread-local tally that is **inert by default** — a
//! resolution attempt is recorded only inside [`record_resolution_attempts`],
//! so the compile/check path pays nothing. The `doctor check type
//! resolution-attempt-determinism` command drives two recorded elaborations
//! and diffs their tallies.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// `Some(tally)` while a [`record_resolution_attempts`] scope is active;
    /// `None` otherwise, which makes [`note_resolution_attempt`] a cheap no-op.
    /// Elaboration is single-threaded (no rayon on the check path), so one
    /// thread-local aggregates every per-module elaborator's attempts.
    static RESOLUTION_ATTEMPTS: RefCell<Option<HashMap<String, usize>>> =
        const { RefCell::new(None) };
}

/// Per-target-name resolution-attempt tally harvested from one recorded
/// elaboration.
pub type ResolutionAttemptTally = HashMap<String, usize>;

/// Run `body` with deferred-resolution-attempt recording enabled, returning
/// its result alongside the per-target-name attempt tally.
///
/// The previous recording state is saved and restored, so this is safe to
/// nest (the diagnostic uses it only at top level). Any attempt noted while
/// `body` runs on this thread is counted.
pub fn record_resolution_attempts<R>(body: impl FnOnce() -> R) -> (R, ResolutionAttemptTally) {
    let previous = RESOLUTION_ATTEMPTS.with(|cell| cell.borrow_mut().replace(HashMap::new()));
    let result = body();
    let tally = RESOLUTION_ATTEMPTS.with(|cell| {
        let mut slot = cell.borrow_mut();
        let taken = slot.take().unwrap_or_default();
        *slot = previous;
        taken
    });
    (result, tally)
}

/// Record one deferred-resolution attempt against `target` (the `@`-stripped
/// name being resolved). A no-op unless a [`record_resolution_attempts`] scope
/// is active on this thread.
pub(crate) fn note_resolution_attempt(target: &str) {
    RESOLUTION_ATTEMPTS.with(|cell| {
        if let Some(tally) = cell.borrow_mut().as_mut() {
            *tally.entry(target.to_string()).or_insert(0) += 1;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_by_default_notes_are_dropped() {
        // Outside a recording scope, noting must not panic or accumulate.
        note_resolution_attempt("Foo");
        let (_, tally) = record_resolution_attempts(|| {});
        assert!(tally.is_empty());
    }

    #[test]
    fn records_and_counts_per_target() {
        let ((), tally) = record_resolution_attempts(|| {
            note_resolution_attempt("Foo");
            note_resolution_attempt("Foo");
            note_resolution_attempt("Bar");
        });
        assert_eq!(tally.get("Foo"), Some(&2));
        assert_eq!(tally.get("Bar"), Some(&1));
        assert_eq!(tally.len(), 2);
    }

    #[test]
    fn scope_restores_previous_recording_state() {
        // A nested scope harvests only its own attempts and restores the outer.
        let (outer_inner_tally, outer_tally) = record_resolution_attempts(|| {
            note_resolution_attempt("Outer");
            let ((), inner) = record_resolution_attempts(|| note_resolution_attempt("Inner"));
            note_resolution_attempt("Outer");
            inner
        });
        assert_eq!(outer_inner_tally.get("Inner"), Some(&1));
        assert!(outer_inner_tally.get("Outer").is_none());
        assert_eq!(outer_tally.get("Outer"), Some(&2));
        assert!(outer_tally.get("Inner").is_none());
    }

    #[test]
    fn recording_is_off_again_after_scope() {
        let _ = record_resolution_attempts(|| note_resolution_attempt("X"));
        note_resolution_attempt("Y"); // must be inert again
        let (_, tally) = record_resolution_attempts(|| {});
        assert!(tally.is_empty());
    }
}
