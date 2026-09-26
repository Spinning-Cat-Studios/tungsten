//! Discovery + transitive closure of needed comparators (ADR 29.6.26f §T11.8).
//!
//! The `__compare` special form emits `Global("compare_T")` calls and records
//! `(symbol, Type)` in the [request registry](super::requests). This pass:
//!
//! 1. scans the given defs for referenced `compare_*` globals that are not yet
//!    defined (the *seed*), resolving each back to its `Type` via the registry;
//! 2. synthesizes a `CoreDef` for each, following the sub-types every composite
//!    comparator references until the set is closed.
//!
//! Output is sorted by symbol so codegen stays deterministic.

use std::collections::HashSet;

use tungsten_core::terms::Term;
use tungsten_core::Type;

use super::context::ComparatorTypes;
use crate::elaborate::CoreDef;

use super::mangling::comparator_symbol;
use super::{requests, synth};

/// Synthesize every comparator transitively reachable from the `compare_T`
/// globals referenced in `defs` but not defined there. Deduplicated by symbol,
/// returned in deterministic (sorted) order. `types` resolves named record types and
/// μ-cluster members during synthesis.
#[must_use]
pub fn synth_missing_comparators<'a>(
    defs: impl IntoIterator<Item = &'a CoreDef>,
    types: &ComparatorTypes,
) -> Vec<CoreDef> {
    let defs: Vec<&CoreDef> = defs.into_iter().collect();
    let defined: HashSet<&str> = defs.iter().map(|d| d.name.as_str()).collect();

    // Seed: top-level referenced comparator symbols → their recorded types.
    let mut referenced: HashSet<String> = HashSet::new();
    for def in &defs {
        collect_comparator_globals(&def.term.term, &mut referenced);
    }
    let mut queue: Vec<Type> = referenced
        .iter()
        .filter(|s| !defined.contains(s.as_str()))
        .filter_map(|s| requests::lookup(s))
        .collect();

    // Transitive closure over sub-types each composite comparator references.
    let mut emitted: HashSet<String> = HashSet::new();
    let mut out: Vec<CoreDef> = Vec::new();
    while let Some(ty) = queue.pop() {
        let symbol = comparator_symbol(&ty);
        if defined.contains(symbol.as_str()) || !emitted.insert(symbol) {
            continue;
        }
        if let Some((defs, subtypes)) = synth::synth_comparator_defs(&ty, types) {
            out.extend(defs);
            queue.extend(subtypes);
        }
    }

    // Sort for deterministic codegen (queue order is not stable).
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Synthesize the comparator for `ty` and its full transitive closure (the
/// sub-comparators it references), seeded directly from a `Type` rather than from
/// `compare_*` globals. Deduplicated by symbol, sorted for determinism. Empty if
/// `ty` is not comparable. Used by the evaluator's lazy `__cmp<T>` resolution
/// (ADR 29.6.26f §T11.2a).
///
/// Unbounded, so it does not terminate on a type whose synthesis diverges
/// (measured: `Expr`, >900 s). Callers that must report rather than wedge use
/// [`synth_closure_bounded`].
#[must_use]
pub fn synth_closure_for(ty: &Type, types: &ComparatorTypes) -> Vec<CoreDef> {
    synth_closure_bounded(ty, types, usize::MAX).defs
}

/// [`synth_closure_for`] under a cap on how many definitions may be emitted.
///
/// Returns the defs and whether the walk **settled** — a `false` second element
/// means the cap was reached with work still queued, so the defs are a
/// truncated prefix and must not be judged as if they were the whole closure
/// (ADR 1.8.26b D3).
///
/// One walk, three callers: the evaluator's gate, `doctor check comparable`,
/// and the unbounded convenience above. Before ADR 1.8.26b the doctor kept its
/// own bounded copy, so the diagnostic and the enforcement could drift.
///
/// # A failed synthesis must not claim the symbol (ADR 1.8.26b)
///
/// `emitted` records only symbols that were actually **defined**. Marking a
/// symbol on *attempt* poisons the walk, because [`comparator_symbol`] is not
/// injective on types: a `Type::Adt` mangles from its name and type arguments
/// alone (`AdtTypeExpr_E`) — deliberately, since the variants are determined by
/// them — so an occurrence still carrying a free μ-bound variable and one that
/// has been closed by substitution share a symbol. Reaching the open one first
/// marked it emitted, its synthesis returned `None`, and the closed one that
/// arrived later was skipped as a duplicate. The closure then *called*
/// `compare_AdtTypeExpr_E` and never defined it — measured on the real
/// `TypeExpr` and `Expr`.
///
/// Termination is unaffected: only a **successful** synthesis enqueues
/// subtypes, so retrying a failing symbol adds no work, while successes are
/// still deduplicated and so cannot cycle.
#[must_use]
pub fn synth_closure_bounded(ty: &Type, types: &ComparatorTypes, cap: usize) -> ClosureWalk {
    let mut queue: Vec<Type> = vec![ty.clone()];
    let mut emitted: HashSet<String> = HashSet::new();
    let mut out: Vec<CoreDef> = Vec::new();
    let mut refused: Vec<Type> = Vec::new();
    let mut converged = true;
    while let Some(t) = queue.pop() {
        if out.len() >= cap {
            converged = false;
            break;
        }
        let symbol = comparator_symbol(&t);
        if emitted.contains(&symbol) {
            continue;
        }
        if let Some((defs, subtypes)) = synth::synth_comparator_defs(&t, types) {
            emitted.insert(symbol);
            out.extend(defs);
            queue.extend(subtypes);
        } else {
            refused.push(t);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    ClosureWalk {
        defs: out,
        converged,
        refused,
    }
}

/// What one closure walk produced.
///
/// `refused` is the half that turns a confusing report into an actionable one:
/// a dangling `compare_AdtExpr_E` reads like a recursion problem, and is in
/// fact whichever sub-type the walk could not synthesize. Keeping the refused
/// types lets the gate say *which* and *why* rather than only *that*.
pub struct ClosureWalk {
    /// The comparators actually defined, sorted by symbol for determinism.
    pub defs: Vec<CoreDef>,
    /// False when the cap was reached with work still queued, so `defs` is a
    /// truncated prefix and must not be judged as a whole closure.
    pub converged: bool,
    /// Types the walk reached and could not synthesize, in encounter order.
    pub refused: Vec<Type>,
}

impl ClosureWalk {
    /// The refused type whose comparator would have been `symbol`, if any.
    #[must_use]
    pub fn refused_under(&self, symbol: &str) -> Option<&Type> {
        self.refused.iter().find(|t| comparator_symbol(t) == symbol)
    }
}

/// Recursively collect `Global` names that name a comparator (`compare_*`).
pub(crate) fn collect_comparator_globals(term: &Term, out: &mut HashSet<String>) {
    if let Term::Global(name) = term {
        if name.starts_with("compare_") {
            out.insert(name.clone());
        }
    }
    term.for_each_subterm(|child| collect_comparator_globals(child, out));
}

#[cfg(test)]
mod tests;
