//! Admission staging and the `#[partial]` taint model
//! (ADR 29.6.26e §2.4, §2.5).
//!
//! Taint is transitive by construction: it is a least fixed point over the
//! occurrence graph seeded from the `#[partial]` annotations, so a definition
//! that merely *wraps* a partial one carries the taint too. That is what stops
//! `def g = f` from laundering a partial `f` into a proof — admission rejects
//! references to *tainted* constants, not only to annotated ones.
//!
//! A definition's **type** is scanned as well as its body: `Eq τ a b` embeds
//! terms, so a theorem statement can mention a constant without its proof term
//! doing so.
//!
//! The fixpoint runs over a [`MentionIndex`] rather than over the definitions
//! directly, so definitions restored from a cache — which have annotations and
//! an adjacency list but no term — participate on equal terms with fresh ones.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::terms::Term;
use crate::types::Type;

use super::graph::OccurrenceGraph;
use super::report::{AdmissionState, FailureReason, TerminationFailure};
use super::{CarriedDefs, DefRole, DefView};

/// Who mentions whom, and which of them are annotated or proof-relevant.
///
/// Built from the freshly elaborated definitions plus whatever a cache carried
/// forward, so the taint fixpoint sees one environment rather than two.
#[derive(Debug, Default)]
pub struct MentionIndex {
    mentions: BTreeMap<String, BTreeSet<String>>,
    partial: BTreeSet<String>,
    proofs: BTreeSet<String>,
}

impl MentionIndex {
    /// Merge the fresh definitions' graph with the carried metadata.
    #[must_use]
    pub fn build(
        defs: &BTreeMap<String, DefView<'_>>,
        graph: &OccurrenceGraph,
        carried: &CarriedDefs,
    ) -> Self {
        let mut index = MentionIndex {
            mentions: carried.mentions.clone(),
            partial: carried.partial.clone(),
            proofs: carried.proofs.clone(),
        };
        for (name, view) in defs {
            let mut reached = graph.mentions(name);
            reached.extend(globals_in_type(view.ty));
            index.mentions.insert(name.clone(), reached);
            if view.annotation.partial {
                index.partial.insert(name.clone());
            }
            if view.role == DefRole::Proof {
                index.proofs.insert(name.clone());
            }
        }
        index
    }

    /// Every name the index knows, fresh or carried.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.mentions.keys()
    }

    /// Whether `name` is annotated `#[partial]`.
    #[must_use]
    pub fn is_partial(&self, name: &str) -> bool {
        self.partial.contains(name)
    }

    /// What `name` mentions, restricted to names the index knows.
    fn reached(&self, name: &str) -> impl Iterator<Item = &String> {
        self.mentions
            .get(name)
            .into_iter()
            .flatten()
            .filter(|target| self.mentions.contains_key(target.as_str()))
    }
}

/// The transitive closure of taint, seeded from `seeds` and the annotations.
///
/// `seeds` carries taint the annotations do not: the members of a recursive
/// group that was skipped because one of them was `#[partial]`.
#[must_use]
pub fn propagate_taint(index: &MentionIndex, seeds: &BTreeSet<String>) -> BTreeSet<String> {
    let mut tainted: BTreeSet<String> = seeds.union(&index.partial).cloned().collect();

    // Least fixed point: re-sweep until a pass adds nothing. The graph is over
    // definitions, not terms, and each pass is one membership test per edge.
    loop {
        let mut grew = false;
        for name in index.names() {
            if tainted.contains(name) {
                continue;
            }
            if index.reached(name).any(|target| tainted.contains(target)) {
                tainted.insert(name.clone());
                grew = true;
            }
        }
        if !grew {
            return tainted;
        }
    }
}

/// Rejections for proofs that reach a tainted constant.
///
/// A proof that is *itself* annotated `#[partial]` is a contradiction in terms
/// and is reported the same way, naming itself.
#[must_use]
pub fn proof_taint_failures(
    index: &MentionIndex,
    tainted: &BTreeSet<String>,
) -> Vec<TerminationFailure> {
    index
        .proofs
        .iter()
        .filter(|name| tainted.contains(name.as_str()))
        .filter_map(|name| {
            let path = taint_path(name, index)?;
            let (seed, via) = path.split_last()?;
            Some(TerminationFailure {
                function: name.clone(),
                group: Vec::new(),
                span: None,
                reason: FailureReason::PartialInProof {
                    tainted: seed.clone(),
                    via: via.to_vec(),
                },
            })
        })
        .collect()
}

/// Final admission state per definition.
#[must_use]
pub fn admission_states(
    index: &MentionIndex,
    tainted: &BTreeSet<String>,
    rejected: &BTreeSet<String>,
) -> BTreeMap<String, AdmissionState> {
    index
        .names()
        .map(|name| {
            let state = if rejected.contains(name) {
                AdmissionState::Rejected
            } else if tainted.contains(name) {
                AdmissionState::Partial
            } else {
                AdmissionState::Total
            };
            (name.clone(), state)
        })
        .collect()
}

/// The shortest chain of constants from `start` to an annotated `#[partial]`
/// definition, excluding `start` itself unless `start` is the seed.
///
/// Breadth-first so the reported chain is the shortest one, which is the one a
/// reader can act on; ties break on name order because the adjacency is sorted.
fn taint_path(start: &str, index: &MentionIndex) -> Option<Vec<String>> {
    if index.is_partial(start) {
        return Some(vec![start.to_string()]);
    }
    let mut seen: BTreeSet<String> = BTreeSet::from([start.to_string()]);
    let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::from([(start.to_string(), vec![])]);

    while let Some((node, prefix)) = queue.pop_front() {
        for target in index.reached(&node) {
            if !seen.insert(target.clone()) {
                continue;
            }
            let mut path = prefix.clone();
            path.push(target.clone());
            if index.is_partial(target) {
                return Some(path);
            }
            queue.push_back((target.clone(), path));
        }
    }
    None
}

/// Global constants embedded in a type — reachable only through the terms an
/// `Eq` witness carries.
///
/// Public because [`MentionIndex::build`] is not the only place a definition
/// has to be reduced to the names taint follows: the self-hosted mirror's seam
/// (ADR 19.8.26d) reduces each definition as it streams, and computing this
/// half itself would be a second walk that could disagree with this one about
/// what an `Eq` witness reaches.
#[must_use]
pub fn globals_in_type(ty: &Type) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    collect_type_globals(ty, &mut found);
    found
}

fn collect_type_globals(ty: &Type, found: &mut BTreeSet<String>) {
    match ty {
        Type::Eq(inner, left, right) => {
            collect_type_globals(inner, found);
            collect_term_globals(left, found);
            collect_term_globals(right, found);
        }
        Type::Arrow(left, right) | Type::Product(left, right) | Type::Sum(left, right) => {
            collect_type_globals(left, found);
            collect_type_globals(right, found);
        }
        Type::Forall(_, inner) | Type::Mu(_, inner) | Type::Ptr(inner) | Type::Ref(inner) => {
            collect_type_globals(inner, found);
        }
        Type::App(_, args) => {
            for arg in args {
                collect_type_globals(arg, found);
            }
        }
        Type::Adt(_, args, variants) => {
            for arg in args {
                collect_type_globals(arg, found);
            }
            for (_, payload) in variants {
                collect_type_globals(payload, found);
            }
        }
        _ => {}
    }
}

fn collect_term_globals(term: &Term, found: &mut BTreeSet<String>) {
    if let Term::Global(name) = term {
        found.insert(name.clone());
    }
    term.for_each_subterm(|child| collect_term_globals(child, found));
}
