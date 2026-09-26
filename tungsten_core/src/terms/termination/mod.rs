//! Termination checking, Phase 1: structural recursion (ADR 29.6.26e).
//!
//! Without this, a non-terminating function inhabits any type — including the
//! empty one — so every proof the compiler accepts is contingent on the author
//! not having written a loop. Phase 1 closes the structural half of that gap: a
//! recursive call must pass a **strict structural subterm**, in the callee's
//! decreasing position, **relative to the caller's decreasing root**.
//!
//! The engine is a pure function of the definitions handed to it. It knows
//! nothing about elaboration, files or diagnostics rendering, so the gate
//! (`elaborate::termination`) and the report-only tool
//! (`doctor check type termination`) run the *same* analysis and cannot
//! disagree.
//!
//! Strict positivity is a hard prerequisite, not an assumption made here: the
//! elaborator's E0061 gate (ADR 7.8.26e) rejects a non-strictly-positive ADT
//! before any definition can reference it, which is what makes "strict
//! subterm" a well-founded relation at all.

mod admission;
mod descent;
mod graph;
mod report;
mod size_env;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use crate::terms::Term;
use crate::types::Type;

pub use admission::{globals_in_type, MentionIndex};
pub use descent::{describe_roots, root_ineligibility, RootCandidacy};
pub use graph::{
    callers_of, invert, is_recursive, reachable_from, tarjan_scc, unreachable_from, Adjacency,
    OccurrenceGraph,
};
pub use report::{
    AdmissionState, FailureReason, RejectedRoot, TerminationFailure, TerminationReport,
};
pub use size_env::{CallSite, SizeClass};

/// What `#[partial]` / `#[decreasing(arg)]` said about a definition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminationAnnotation {
    /// `#[partial]`: opt out of the check, and carry taint.
    pub partial: bool,
    /// `#[decreasing(arg)]`: the parameter *name* that decreases.
    pub decreasing: Option<String>,
}

/// Whether a definition is proof-relevant.
///
/// The distinction is the whole point of taint: a partial constant is welcome
/// in executable code and inadmissible in a proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefRole {
    /// An ordinary function — may depend on partial definitions.
    Executable,
    /// A theorem, lemma or other trusted proof artifact.
    Proof,
}

/// One definition, as the checker sees it.
#[derive(Debug, Clone, Copy)]
pub struct DefView<'a> {
    /// The definition's type — scanned for taint, because `Eq` embeds terms.
    pub ty: &'a Type,
    /// The elaborated body.
    pub term: &'a Term,
    /// Its termination annotations.
    pub annotation: &'a TerminationAnnotation,
    /// Whether it is proof-relevant.
    pub role: DefRole,
}

/// Definitions the analysis knows about but has no term for.
///
/// A module served from the elaboration cache contributes no `CoreDef`s, so
/// without this its definitions would vanish from the environment and a proof
/// could reach a partial constant across the warm/cold boundary unnoticed.
/// Descent does not need them — a value SCC never spans modules, because
/// modules elaborate in post-order over an acyclic import graph, so every
/// recursive group is wholly inside the module that was checked when it was
/// fresh. Taint does, and taint needs only names and edges.
#[derive(Debug, Clone, Default)]
pub struct CarriedDefs {
    /// Definition → the definitions it mentions.
    pub mentions: BTreeMap<String, BTreeSet<String>>,
    /// Which of them are annotated `#[partial]`.
    pub partial: BTreeSet<String>,
    /// Which of them are proof-relevant.
    pub proofs: BTreeSet<String>,
}

/// Run the whole Phase-1 analysis over a set of definitions.
///
/// Every definition traverses the same admission state machine regardless of
/// where it came from — fresh elaboration, an import, or a deserialized cache
/// entry — because the input is the assembled definition set, not a source
/// file (ADR 29.6.26e §2.5).
#[must_use]
pub fn analyze(defs: &BTreeMap<String, DefView<'_>>) -> TerminationReport {
    analyze_with_carried(defs, &CarriedDefs::default())
}

/// [`analyze`], with definitions restored from a cache participating in taint.
#[must_use]
pub fn analyze_with_carried(
    defs: &BTreeMap<String, DefView<'_>>,
    carried: &CarriedDefs,
) -> TerminationReport {
    let carried_names: BTreeSet<String> = carried.mentions.keys().cloned().collect();
    let graph = OccurrenceGraph::build_knowing(
        defs.iter().map(|(name, view)| (name.as_str(), view.term)),
        &carried_names,
    );
    let adjacency = graph.adjacency();

    let mut recursive_groups = Vec::new();
    let mut failures = Vec::new();
    let mut rejected: BTreeSet<String> = BTreeSet::new();
    let mut seeds: BTreeSet<String> = BTreeSet::new();

    for component in tarjan_scc(&adjacency) {
        if !is_recursive(&component, &adjacency) {
            continue;
        }
        recursive_groups.push(component.clone());

        // A group with any `#[partial]` member is unchecked as a whole: the
        // recursion runs through the annotated member, so certifying the others
        // in isolation would certify nothing.
        if component.iter().any(|name| defs[name].annotation.partial) {
            seeds.extend(component.iter().cloned());
            continue;
        }

        let group_failures = descent::check_group(&component, defs, &graph);
        if !group_failures.is_empty() {
            rejected.extend(component.iter().cloned());
            failures.extend(group_failures);
        }
    }

    let index = admission::MentionIndex::build(defs, &graph, carried);
    let tainted = admission::propagate_taint(&index, &seeds);
    failures.extend(admission::proof_taint_failures(&index, &tainted));

    TerminationReport {
        admission: admission::admission_states(&index, &tainted, &rejected),
        tainted: tainted.into_iter().collect(),
        recursive_groups,
        failures,
    }
}
