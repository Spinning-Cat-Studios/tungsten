//! The termination gate: admission at the trusted boundary (ADR 29.6.26e).
//!
//! The analysis itself is `tungsten_core::terms::termination`; this module is
//! the adapter. It builds the engine's input from an assembled definition set
//! plus the annotations the elaborator recorded, and turns the engine's
//! failures into `ElabError`s.
//!
//! **Where it runs matters.** The gate is applied to the whole project's
//! definitions, once, after every module has been elaborated — not per module
//! during Body Elaboration. That is the only point at which imported and
//! cache-reconstructed definitions are in the same set as freshly elaborated
//! ones, and admitting them by separate routes is exactly the hole the ADR
//! exists to close.
//!
//! The elaboration cache is the one route that would otherwise slip past it: a
//! signature-cache hit skips body elaboration, so the module's `CoreDef`s are
//! simply absent and a whole-project gate would certify a set that no longer
//! contains them. [`CachedTermination`] closes that — the entry carries the
//! module's verdict and the names and edges taint needs, and the gate replays
//! both.

mod cache;
mod rollout;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use tungsten_core::terms::termination::{
    analyze_with_carried, CarriedDefs, DefRole, DefView, FailureReason, OccurrenceGraph,
    TerminationAnnotation, TerminationFailure, TerminationReport,
};

use crate::span::Span;

use super::{CoreDef, DefTerminationMeta, ElabError, ElabErrorKind};

pub use cache::CachedTermination;
pub use rollout::{enforcement, reset_enforcement, set_enforcement, Enforcement, ReportingOnly};

/// Re-exported so every test that touches the process-wide enforcement level
/// can take the *same* lock, wherever in the crate it lives (ADR 12.8.26a §5.1).
#[cfg(test)]
pub(crate) use rollout::lock_enforcement;

/// The annotations, converted once so the borrowed views can point at them.
///
/// The engine takes `&TerminationAnnotation`, and the elaborator stores
/// `DefTerminationMeta`; this owns the converted values for the duration of a
/// check.
pub struct TerminationInput {
    annotations: BTreeMap<String, TerminationAnnotation>,
    proofs: BTreeMap<String, bool>,
}

impl TerminationInput {
    /// Convert the elaborator's per-definition metadata.
    #[must_use]
    pub fn from_meta(meta: &HashMap<String, DefTerminationMeta>) -> Self {
        TerminationInput {
            annotations: meta
                .iter()
                .map(|(name, entry)| {
                    (
                        name.clone(),
                        TerminationAnnotation {
                            partial: entry.attrs.partial,
                            decreasing: entry
                                .attrs
                                .decreasing
                                .as_ref()
                                .map(|ident| ident.name.clone()),
                        },
                    )
                })
                .collect(),
            proofs: meta
                .iter()
                .map(|(name, entry)| (name.clone(), entry.is_proof))
                .collect(),
        }
    }

    /// Run the analysis over `defs`.
    ///
    /// A definition with no recorded metadata is executable and unannotated —
    /// the overwhelmingly common case, which is why the elaborator only records
    /// the exceptions.
    #[must_use]
    pub fn check(&self, defs: &[CoreDef]) -> TerminationReport {
        self.check_with_carried(defs, &CarriedDefs::default())
    }

    /// [`TerminationInput::check`], with cache-carried definitions taking part
    /// in taint propagation.
    #[must_use]
    pub fn check_with_carried(&self, defs: &[CoreDef], carried: &CarriedDefs) -> TerminationReport {
        let unannotated = TerminationAnnotation::default();
        let views: BTreeMap<String, DefView<'_>> = defs
            .iter()
            .map(|def| {
                (
                    def.name.clone(),
                    DefView {
                        ty: &def.ty,
                        term: &def.term.term,
                        annotation: self.annotations.get(&def.name).unwrap_or(&unannotated),
                        role: if self.proofs.get(&def.name).copied().unwrap_or(false) {
                            DefRole::Proof
                        } else {
                            DefRole::Executable
                        },
                    },
                )
            })
            .collect();
        analyze_with_carried(&views, carried)
    }

    /// Whether `name` is proof-relevant.
    #[must_use]
    pub fn is_proof(&self, name: &str) -> bool {
        self.proofs.get(name).copied().unwrap_or(false)
    }

    /// Split a report's rejections into the ones that fail the build and the
    /// ones that are only reported, at `level`.
    ///
    /// A failure is proof-relevant when the rejected definition is itself a
    /// proof, or when the reason *is* the proof boundary — the two ways a
    /// rejection can matter to soundness rather than only to confidence.
    #[must_use]
    pub fn partition_errors(
        &self,
        report: &TerminationReport,
        defs: &[CoreDef],
        level: Enforcement,
    ) -> AdmissionSplit {
        let mut split = AdmissionSplit::default();
        for (failure, error) in report.failures.iter().zip(admission_errors(report, defs)) {
            let proof_relevant = self.is_proof(&failure.function)
                || matches!(failure.reason, FailureReason::PartialInProof { .. });
            split.push(error, proof_relevant, level);
        }
        split
    }
}

/// Rejections split by whether they fail the build.
#[derive(Debug, Default)]
pub struct AdmissionSplit {
    /// Errors that abort elaboration.
    pub gating: Vec<ElabError>,
    /// Errors demoted to warnings at the current enforcement level.
    pub reported: Vec<ElabError>,
}

/// Whether a report plus whatever a cache carried has nothing to say.
///
/// A free function so the two halves of the condition are assertable: a report
/// that is clean says nothing about rejections a *cached* module already
/// recorded, and reading only the first half is how those get dropped on every
/// run after the first.
#[must_use]
pub fn has_nothing_to_report(report: &TerminationReport, carried: &CachedTermination) -> bool {
    report.is_clean() && carried.failures.is_empty()
}

impl AdmissionSplit {
    /// Add one already-rendered rejection at `level`.
    pub fn push(&mut self, error: ElabError, proof_relevant: bool, level: Enforcement) {
        if level.gates(proof_relevant) {
            self.gating.push(error);
        } else {
            self.reported.push(error);
        }
    }
}

/// Render a report's failures as elaboration errors, pointed at source.
///
/// `defs` supplies the fallback span: a failure about a whole definition (no
/// decreasing parameter, an ambiguity, a taint chain) has no single call site
/// to blame, and pointing at the definition is better than pointing at byte 0.
#[must_use]
pub fn admission_errors(report: &TerminationReport, defs: &[CoreDef]) -> Vec<ElabError> {
    let spans: BTreeMap<&str, Span> = defs
        .iter()
        .map(|def| (def.name.as_str(), def.span))
        .collect();
    report
        .failures
        .iter()
        .map(|failure| {
            let span = failure
                .span
                .map(|term_span| Span::new(term_span.start, term_span.end))
                .or_else(|| spans.get(failure.function.as_str()).copied())
                .unwrap_or_default();
            build_error(failure, span)
        })
        .collect()
}

/// One failure as an `ElabError`, with the engine's notes and help attached.
fn build_error(failure: &TerminationFailure, span: Span) -> ElabError {
    let kind = match &failure.reason {
        FailureReason::PartialInProof { tainted, .. } => ElabErrorKind::PartialInProof {
            proof: failure.function.clone(),
            tainted: tainted.clone(),
        },
        _ => ElabErrorKind::CannotProveTermination {
            function: failure.function.clone(),
            headline: failure.headline(),
        },
    };
    let mut error = ElabError::new(span, kind);
    for note in failure.notes() {
        error = error.with_note(note);
    }
    match failure.suggestion() {
        Some(help) => error.with_help(help),
        None => error,
    }
}
