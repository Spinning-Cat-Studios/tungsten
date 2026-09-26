//! Decreasing-parameter selection and the descent check
//! (ADR 29.6.26e §2.2, §2.3).
//!
//! Each member of a recursive group gets **one** decreasing parameter. Descent
//! is measured relative to the *caller's* root: for an intra-group call
//! `F → G`, the argument `F` supplies in `G`'s decreasing position must be a
//! known strict subterm of `F`'s own decreasing parameter. Occupying `G`'s
//! decreasing position is not enough — that would let a group make no progress
//! while every individual call looked plausible.
//!
//! Because the members' choices constrain each other, they are made together:
//! the checker searches assignments of one candidate position per member and
//! accepts the group when some assignment satisfies every call. Inference only
//! stands when the search leaves the choice determined; otherwise
//! `#[decreasing(arg)]` is required.
//!
//! The three steps are one file each: [`roots`] decides what a member may
//! descend on, [`search`] enumerates the assignments, and [`failures`] turns a
//! search that found nothing into rejections a reader can act on.

mod failures;
mod roots;
mod search;

use std::collections::BTreeMap;

use crate::terms::Term;
use crate::types::Type;

use super::graph::{transparent, OccurrenceGraph};
use super::report::TerminationFailure;
use super::size_env::{collect_call_sites, CallSite};
use super::DefView;

pub use roots::{describe_roots, root_ineligibility, RootCandidacy};

use failures::{ambiguity_failure, descent_failures, undetermined_failures};
use roots::{candidate_positions, indirect_failures};
use search::search_assignments;

/// Assignment-search ceiling. Past this the group is treated as needing
/// explicit annotations rather than searched exhaustively — a group large
/// enough to exceed it is one no reader could follow either.
const MAX_ASSIGNMENTS: usize = 4096;

/// A function's value parameters and the body beneath them.
pub(super) struct Signature<'a> {
    pub(super) names: Vec<String>,
    pub(super) types: Vec<&'a Type>,
    /// The `TyAbs` binders peeled on the way in: this definition's **own**
    /// generic parameters, and so the only type variables that are genuinely
    /// abstract inside it. Named for what they are rather than where they came
    /// from — "binders" reads as a position in the term, and the property that
    /// matters is ownership: a `TyVar` this definition binds is an abstract `T`,
    /// and one it does not is a name from the environment.
    pub(super) own_type_parameters: Vec<String>,
    body: &'a Term,
}

/// Peel type abstractions and value lambdas off a definition.
///
/// The peeling is positional and matches how [`super::size_env`] indexes call
/// arguments, so parameter *i* here is the argument at spine position *i*
/// there. A function returning a closure has that closure's binder peeled too;
/// that only ever adds a candidate position, and a candidate that is not
/// supplied by any recursive call is rejected rather than assumed.
///
/// The `TyAbs` binder *names* are kept, not just skipped: they are what
/// separates an abstract `T` from a nominal type name that reached Core as a
/// bare `TyVar` (see [`is_supported_inductive`]).
pub(super) fn signature(term: &Term) -> Signature<'_> {
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut own_type_parameters = Vec::new();
    // `cursor` stays *unstripped* so the body keeps its `Spanned` wrapper —
    // that wrapper is where a call-site diagnostic gets its span from.
    let mut cursor = term;
    loop {
        match transparent(cursor) {
            Term::TyAbs(binder, inner) => {
                own_type_parameters.push(binder.clone());
                cursor = inner;
            }
            Term::Lambda(name, ty, inner) => {
                names.push(name.clone());
                types.push(ty);
                cursor = inner;
            }
            _ => break,
        }
    }
    Signature {
        names,
        types,
        own_type_parameters,
        body: cursor,
    }
}

/// Whether Phase 1 can descend on a parameter of this type.
///
/// The supported subset is simple strictly-positive ADTs: the μ-encoding of a
/// recursive one, the flat representation of a three-or-more-constructor one,
/// or — see below — a **nominal marker** for a member of a mutually recursive
/// type cluster. Everything else — `Nat`, `String`, arrows, references,
/// unresolved applications, abstract type parameters — is conservatively
/// rejected rather than silently accepted (ADR 29.6.26e §2.1, Non-Goals).
///
/// **Why a bare `TyVar` can be a descent root (ADR 11.8.26b).** A mutually
/// recursive type cluster μ-encodes with the nested binders all wrapping the
/// *entry* member's body, so a non-entry member reaches Core as the bare marker
/// `TyVar("Member")` rather than as its own `Mu`. Rejecting every `TyVar`
/// therefore rejected genuine structural recursion over an inductive type —
/// measured at 43 of the self-hosted compiler's definitions, the `strip_spans_*`
/// AST walk chief among them — for which neither a rewrite nor an honest
/// `#[partial]` exists.
///
/// `own_type_parameters` are the definition's own `TyAbs` names, and they are
/// the whole discriminator: a `TyVar` bound by one is an abstract `T` and stays
/// rejected;
/// an unbound one names something nominal. Widening the *candidate* set cannot
/// certify anything on its own — descent must still be proved by a `Fst`/`Snd`
/// or a match payload of the root, which an abstract parameter admits neither
/// of, so the conservative answer survives where it was load-bearing.
///
/// Guarded by `a_bare_tyvar_is_a_root_only_when_it_is_not_a_type_parameter`
/// (the predicate) and the golden fixture
/// `tests/golden/check/termination_mutual_cluster_record_member.tg` (the class
/// — narrowing this back to `Mu | Adt` turns that file's clean census into two
/// rejections, which no predicate test can show).
#[must_use]
pub fn is_supported_inductive(ty: &Type, own_type_parameters: &[String]) -> bool {
    match ty {
        Type::Mu(..) | Type::Adt(..) => true,
        Type::TyVar(name) => !own_type_parameters
            .iter()
            .any(|parameter| parameter == name.as_str()),
        _ => false,
    }
}

/// Check one recursive group, returning its rejections (empty when it passes).
#[must_use]
pub fn check_group(
    group: &[String],
    defs: &BTreeMap<String, DefView<'_>>,
    graph: &OccurrenceGraph,
) -> Vec<TerminationFailure> {
    let indirect = indirect_failures(group, graph);
    if !indirect.is_empty() {
        return indirect;
    }

    let signatures: Vec<Signature<'_>> = group
        .iter()
        .map(|name| signature(defs[name].term))
        .collect();

    let candidates = match candidate_positions(group, defs, &signatures) {
        Ok(candidates) => candidates,
        Err(failures) => return failures,
    };

    // One call-site collection per (member, candidate root). The walk depends
    // on the root, so it cannot be shared across candidates.
    let sites: Vec<BTreeMap<usize, Vec<CallSite>>> = group
        .iter()
        .enumerate()
        .map(|(member, _)| {
            candidates[member]
                .iter()
                .map(|&position| {
                    (
                        position,
                        collect_call_sites(
                            signatures[member].body,
                            &signatures[member].names[position],
                            group,
                        ),
                    )
                })
                .collect()
        })
        .collect();

    if exceeds_search_ceiling(&candidates) {
        return vec![ambiguity_failure(group, &signatures, &candidates, 0)];
    }

    let satisfying = search_assignments(group, &candidates, &sites);
    if satisfying.is_empty() {
        return descent_failures(group, &signatures, &candidates, &sites);
    }
    undetermined_failures(group, defs, &signatures, &candidates, &satisfying)
}

/// Whether the assignment product is too large to enumerate.
///
/// A separate predicate rather than an inline comparison so the boundary is
/// reachable from a test: constructing a real group with 4097 assignments would
/// need a mutual group of six five-parameter functions, which proves nothing the
/// arithmetic does not.
///
/// The product **saturates**. A plain `product()` overflows on a group with a
/// hundred-odd members each carrying two candidates — which panics in debug and,
/// far worse, *wraps* in release, so a group far past the ceiling can wrap to a
/// small total and be enumerated instead of rejected. The self-hosted parser's
/// 129-member SCC is that shape as soon as its members have candidates at all.
#[must_use]
pub fn exceeds_search_ceiling(candidates: &[Vec<usize>]) -> bool {
    candidates
        .iter()
        .try_fold(1usize, |total, positions| {
            let next = total.saturating_mul(positions.len());
            (next <= MAX_ASSIGNMENTS).then_some(next)
        })
        .is_none()
}
