//! Who calls whom, and what nothing calls at all.
//!
//! [`Adjacency`] answers "what does this definition call" directly — it is
//! keyed that way. The two questions below are the ones it does *not* answer
//! without work, and both came out of ADR 12.8.26b, where a `pub use`-exported
//! function with **zero callers** was mistaken for the live one and an ADR was
//! written against it. Grep cannot settle that: an export looks exactly like a
//! call site, and in `.tg` the re-export is usually in a different file from
//! both.
//!
//! Everything here is a pure function over the adjacency, so it is assertable
//! without elaborating anything. The callers build the graph
//! ([`super::OccurrenceGraph`]) and hand it over.
//!
//! **Absent keys are nodes too.** An adjacency maps *definitions* to the names
//! they reference, and a referenced name that is not itself a definition (an
//! extern, a builtin, a constructor) has no key. `tarjan_scc` ignores those by
//! documented design — the caller owns the node set — and so does this module:
//! the node set is the key set, never the union of keys and targets. Reading it
//! the other way would report every builtin as an uncalled definition.

use std::collections::{BTreeMap, BTreeSet};

use super::scc::Adjacency;

/// Every definition that references `target`.
///
/// Self-recursion counts: a function that calls only itself has itself as a
/// caller, which is what distinguishes "recursive but unreachable" from "not
/// called at all" — the first still needs an entry point.
#[must_use]
pub fn callers_of(adjacency: &Adjacency, target: &str) -> BTreeSet<String> {
    adjacency
        .iter()
        .filter(|(_, callees)| callees.contains(target))
        .map(|(caller, _)| caller.clone())
        .collect()
}

/// The whole caller relation, inverted once.
///
/// Cheaper than calling [`callers_of`] per definition when the answer is wanted
/// for every node — which is what a census does. Definitions with no callers
/// appear with an empty set rather than being absent, so a consumer can tell
/// "nothing calls it" from "not a definition".
#[must_use]
pub fn invert(adjacency: &Adjacency) -> BTreeMap<String, BTreeSet<String>> {
    let mut inverted: BTreeMap<String, BTreeSet<String>> = adjacency
        .keys()
        .map(|name| (name.clone(), BTreeSet::new()))
        .collect();
    for (caller, callees) in adjacency {
        for callee in callees {
            // Only definitions get rows; see the module note on absent keys.
            if let Some(row) = inverted.get_mut(callee) {
                row.insert(caller.clone());
            }
        }
    }
    inverted
}

/// Every definition reachable from `roots`, the roots included when they are
/// definitions.
///
/// Breadth-first over the adjacency. A root that is not a definition
/// contributes nothing rather than erroring: the caller's root set is a
/// *policy* (`main`, the `test_*` convention), and a policy naming something
/// absent from this file is normal.
#[must_use]
pub fn reachable_from(adjacency: &Adjacency, roots: &BTreeSet<String>) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = roots
        .iter()
        .filter(|root| adjacency.contains_key(*root))
        .cloned()
        .collect();
    for root in &queue {
        seen.insert(root.clone());
    }

    while let Some(current) = queue.pop() {
        let Some(callees) = adjacency.get(&current) else {
            continue;
        };
        for callee in callees {
            // `insert` returns false for an already-seen node, which is what
            // terminates this on a cyclic graph — every real call graph here
            // has cycles, so the check is load-bearing, not a micro-optimisation.
            if adjacency.contains_key(callee) && seen.insert(callee.clone()) {
                queue.push(callee.clone());
            }
        }
    }
    seen
}

/// Definitions no path from `roots` reaches.
///
/// The complement of [`reachable_from`] over the definition set. Note what this
/// is *not*: it is not "has no callers". A mutually recursive pair that nothing
/// else calls has callers — each other — and is still unreachable, which is
/// exactly the shape a dead-code census must catch and a caller count cannot.
#[must_use]
pub fn unreachable_from(adjacency: &Adjacency, roots: &BTreeSet<String>) -> BTreeSet<String> {
    let reachable = reachable_from(adjacency, roots);
    adjacency
        .keys()
        .filter(|name| !reachable.contains(*name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
