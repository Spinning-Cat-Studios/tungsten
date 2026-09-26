//! Strict-positivity gate: the elaborator's node-E driver (ADR 7.8.26e §2.2).
//!
//! Runs immediately after Recursion Grouping — the first point at which a
//! complete type graph exists, and before Deferred-TyVar Resolution rewrites
//! the `@`-prefixed cross-references the walker reads. It is **not** a
//! `PhaseCheckResult`: those are opt-in diagnostics gated on
//! `check_phase_invariants`, and a soundness gate must not be optional.
//!
//! Imports and cache-reconstructed definitions need no separate arm — they are
//! in `env.types` by the time this runs, so the all-definitions worklist covers
//! them, in their true SCCs, with the same code.

mod build;

use std::collections::{BTreeSet, HashSet};

use tungsten_core::types::positivity::{
    check_strict_positivity, head_census, param_occurrences, referenced_names, HeadCensus,
    ParamOccs, PositivityDefs, PositivityViolation,
};

use crate::doctor::audit_mutual_types::scc::tarjan_scc_with_depth;
use crate::doctor::audit_mutual_types::type_graph::TypeGraph;
use crate::elaborate::{ElabError, ElabErrorKind, Elaborator};
use crate::span::Span;

pub use build::{from_env_types, from_project, SpanIndex};

/// Everything one node-E pass computed, for the elaborator hook and for
/// `doctor check type positivity` alike.
pub struct PositivityReport {
    /// Violations, in deterministic order (group members sorted, then
    /// constructor and field order).
    pub violations: Vec<PositivityViolation>,
    /// Per-type parameter strictness, computed once over the whole graph.
    pub param_occs: ParamOccs,
    /// Every SCC of the expanded graph, singletons included.
    pub groups: Vec<BTreeSet<String>>,
    /// Unresolved `App`/`Adt` heads, split by `Stub` vs genuinely absent (D6).
    pub census: HeadCensus,
    /// Deepest `tarjan_scc` recursion reached — the expanded node set is larger
    /// than the elaborator's ADT-only one, and `strongconnect` recurses.
    pub max_tarjan_depth: usize,
    /// Where each definition is reported, keyed by type name.
    spans: SpanIndex,
}

impl PositivityReport {
    /// The span to report a violation in `type_name` at.
    #[must_use]
    pub fn span_of(&self, type_name: &str) -> Span {
        self.spans.get(type_name).copied().unwrap_or_default()
    }

    /// The largest SCC in the expanded graph.
    #[must_use]
    pub fn max_group_size(&self) -> usize {
        self.groups.iter().map(BTreeSet::len).max().unwrap_or(0)
    }
}

/// Run the whole node-E analysis over an already-built engine input.
///
/// Split out from [`Elaborator::check_strict_positivity`] so it is a pure
/// function of `(defs, spans)` — the elaborator supplies those, and nothing
/// here touches the environment.
#[must_use]
pub fn analyze(defs: &PositivityDefs, spans: SpanIndex) -> PositivityReport {
    // The fixpoint is over the whole type graph, so it is computed once per
    // pass rather than once per group (which would be quadratic for no gain).
    let param_occs = param_occurrences(defs);

    let adjacency = referenced_names(defs)
        .into_iter()
        .map(|(name, refs)| (name, refs.into_iter().collect::<HashSet<_>>()));
    let graph = TypeGraph::from_adjacency(adjacency);
    let (sccs, max_tarjan_depth) = tarjan_scc_with_depth(&graph);

    // EVERY definition is checked, in its own group — singletons included.
    // `type Bad = Mk(Bad -> Bad)` is a size-1 SCC and is therefore absent from
    // the elaborator's `mutual_recursion_groups`, which stores only SCCs of
    // size > 1; reading that map as a worklist checks nothing in this case.
    let groups: Vec<BTreeSet<String>> = sccs
        .into_iter()
        .map(|scc| scc.into_iter().collect())
        .collect();

    let violations = groups
        .iter()
        .flat_map(|group| check_strict_positivity(group, defs, &param_occs))
        .collect();

    PositivityReport {
        violations,
        param_occs,
        groups,
        census: head_census(defs),
        max_tarjan_depth,
        spans,
    }
}

/// Render one violation as an `ElabError` (E0061).
///
/// The `note:` lines **are** `via`'s rendering, not a third channel: for an
/// inherited violation the reader cannot see the forbidden position in their
/// own source, so the intermediate type and parameter have to be named.
#[must_use]
pub fn violation_error(violation: &PositivityViolation, span: Span) -> ElabError {
    let field = violation.field.to_string();
    let kind = ElabErrorKind::NonStrictlyPositive {
        type_name: violation.type_name.clone(),
        ctor_name: violation.ctor_name.clone(),
        is_record: violation.is_record,
        field: field.clone(),
        occurrence: violation.occurrence.clone(),
        via: violation
            .via
            .iter()
            .map(|link| (link.type_name.clone(), link.param.clone()))
            .collect(),
    };
    let mut error = ElabError::new(span, kind);
    for link in &violation.via {
        error = error.with_note(format!(
            "`{}` does not use its parameter `{}` strictly positively",
            link.type_name, link.param
        ));
    }
    error
        .with_note(
            "strict positivity is required for structural recursion to be well-founded".to_string(),
        )
        .with_help(
            "move the occurrence out of the argument position — store the \
             result of the function rather than the function itself"
                .to_string(),
        )
}

impl Elaborator<'_> {
    /// Node E: reject type definitions whose own SCC is reachable from a
    /// forbidden position.
    pub(super) fn check_strict_positivity(&mut self) {
        let report = self.positivity_report();

        // `mutual_recursion_groups` is a debug cross-check only, never the
        // worklist: a mismatch means OUR graph is wrong, and emitting a
        // user-facing soundness error on a compiler-internal disagreement is
        // the cascade this gate refuses elsewhere.
        debug_assert!(
            groups_are_subsumed(self.mutual_recursion_groups.values(), &report.groups),
            "elaborator mutual-recursion groups are not a subset of the \
             strict-positivity SCCs — the positivity type graph is missing edges"
        );

        for violation in &report.violations {
            let span = report.span_of(&violation.type_name);
            self.record_error(violation_error(violation, span));
        }
    }

    /// Build the engine input from `env.types` and run the analysis.
    ///
    /// Also the entry point for `doctor check type positivity`, so the
    /// report-only tool and the gate cannot disagree.
    pub fn positivity_report(&self) -> PositivityReport {
        let (defs, spans) = from_env_types(self.env.types.iter());
        analyze(&defs, spans)
    }
}

/// Whether every elaborator mutual-recursion group is contained in one of our
/// SCCs (the D3 cross-check).
///
/// A free function over injected data rather than an `Elaborator` method,
/// because its only caller is a `debug_assert!` — which is compiled out of a
/// release build and so cannot be asserted on there.
pub fn groups_are_subsumed<'a>(
    elaborator_groups: impl Iterator<Item = &'a Vec<String>>,
    sccs: &[BTreeSet<String>],
) -> bool {
    elaborator_groups.into_iter().all(|members| {
        sccs.iter()
            .any(|scc| members.iter().all(|member| scc.contains(member)))
    })
}

#[cfg(test)]
mod tests;
