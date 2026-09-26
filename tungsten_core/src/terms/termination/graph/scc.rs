//! Tarjan's algorithm over a name-keyed adjacency map.
//!
//! Shared by the termination gate and `doctor audit-recursion`, so the report
//! and the gate cannot disagree about what a recursive group is. Components
//! come back in reverse topological order (leaves first), each sorted, and the
//! iteration order of the adjacency is deterministic because it is a
//! `BTreeMap`/`BTreeSet` — an SCC list that reordered between runs would make
//! every downstream diagnostic unstable.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Name-keyed adjacency: node → the nodes it points at.
pub type Adjacency = BTreeMap<String, BTreeSet<String>>;

/// Find all strongly connected components of `adjacency`.
///
/// Nodes absent from `adjacency` as keys but present as targets are ignored:
/// the caller owns the node set, and an edge to a name that is not a definition
/// (an extern, a builtin) is not a recursion.
#[must_use]
pub fn tarjan_scc(adjacency: &Adjacency) -> Vec<Vec<String>> {
    let mut state = TarjanState {
        next_index: 0,
        stack: Vec::new(),
        on_stack: HashMap::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        components: Vec::new(),
    };

    for node in adjacency.keys() {
        if !state.indices.contains_key(node.as_str()) {
            strongconnect(node, adjacency, &mut state);
        }
    }

    state.components
}

/// Whether `component` is recursive: a group of two or more, or a singleton
/// that points at itself.
///
/// A singleton *without* a self-edge is an ordinary definition and needs no
/// decreasing parameter (ADR 29.6.26e § Applies only to recursive SCCs).
#[must_use]
pub fn is_recursive(component: &[String], adjacency: &Adjacency) -> bool {
    if component.len() > 1 {
        return true;
    }
    component
        .first()
        .is_some_and(|only| adjacency.get(only).is_some_and(|out| out.contains(only)))
}

struct TarjanState {
    next_index: usize,
    stack: Vec<String>,
    on_stack: HashMap<String, bool>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    components: Vec<Vec<String>>,
}

fn strongconnect(node: &str, adjacency: &Adjacency, state: &mut TarjanState) {
    let index = state.next_index;
    state.next_index += 1;
    state.indices.insert(node.to_string(), index);
    state.lowlinks.insert(node.to_string(), index);
    state.stack.push(node.to_string());
    state.on_stack.insert(node.to_string(), true);

    if let Some(successors) = adjacency.get(node) {
        for successor in successors {
            if !adjacency.contains_key(successor.as_str()) {
                continue; // Edge to a name that is not a definition.
            }
            if !state.indices.contains_key(successor.as_str()) {
                strongconnect(successor, adjacency, state);
                relax(node, state.lowlinks[successor.as_str()], state);
            } else if state.on_stack.get(successor.as_str()).copied() == Some(true) {
                relax(node, state.indices[successor.as_str()], state);
            }
        }
    }

    if state.lowlinks[node] == state.indices[node] {
        let mut component = Vec::new();
        loop {
            let popped = state
                .stack
                .pop()
                .expect("Tarjan stack is non-empty at a root");
            state.on_stack.insert(popped.clone(), false);
            let is_root = popped == node;
            component.push(popped);
            if is_root {
                break;
            }
        }
        component.sort();
        state.components.push(component);
    }
}

/// Lower `node`'s lowlink to `candidate` if that is smaller.
fn relax(node: &str, candidate: usize, state: &mut TarjanState) {
    let lowlink = state
        .lowlinks
        .get_mut(node)
        .expect("lowlink is set on entry to strongconnect");
    // `min` rather than a compare-and-assign: writing an equal value is a no-op,
    // so a `<` / `<=` slip here would be observationally identical and no test
    // could ever distinguish it.
    *lowlink = (*lowlink).min(candidate);
}
