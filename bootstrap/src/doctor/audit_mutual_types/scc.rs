//! Tarjan's algorithm for finding strongly connected components in the type graph.

use std::collections::HashMap;

use super::type_graph::TypeGraph;

/// Find all strongly connected components using Tarjan's algorithm.
///
/// Returns components in reverse topological order (leaf SCCs first).
pub fn tarjan_scc(graph: &TypeGraph) -> Vec<Vec<String>> {
    tarjan_scc_with_depth(graph).0
}

/// [`tarjan_scc`], additionally reporting the deepest `strongconnect` recursion
/// reached.
///
/// `strongconnect` recurses, so a graph whose node set is larger than the
/// elaborator's ADT-only one (as the strict-positivity driver's is —
/// ADR 7.8.26e D3) needs the depth measured rather than assumed.
pub fn tarjan_scc_with_depth(graph: &TypeGraph) -> (Vec<Vec<String>>, usize) {
    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashMap::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        result: Vec::new(),
        depth: 0,
        max_depth: 0,
    };

    let mut nodes: Vec<&String> = graph.nodes().iter().collect();
    nodes.sort(); // Deterministic ordering

    for node in nodes {
        if !state.indices.contains_key(node.as_str()) {
            strongconnect(node, graph, &mut state);
        }
    }

    (state.result, state.max_depth)
}

struct TarjanState {
    index_counter: usize,
    stack: Vec<String>,
    on_stack: HashMap<String, bool>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    result: Vec<Vec<String>>,
    /// Current `strongconnect` recursion depth.
    depth: usize,
    /// Deepest recursion reached over the whole run.
    max_depth: usize,
}

fn strongconnect(v: &str, graph: &TypeGraph, state: &mut TarjanState) {
    state.depth += 1;
    state.max_depth = state.max_depth.max(state.depth);
    strongconnect_inner(v, graph, state);
    state.depth -= 1;
}

fn strongconnect_inner(v: &str, graph: &TypeGraph, state: &mut TarjanState) {
    let v_index = state.index_counter;
    state.index_counter += 1;
    state.indices.insert(v.to_string(), v_index);
    state.lowlinks.insert(v.to_string(), v_index);
    state.stack.push(v.to_string());
    state.on_stack.insert(v.to_string(), true);

    if let Some(deps) = graph.callees(v) {
        let mut sorted_deps: Vec<&String> = deps.iter().collect();
        sorted_deps.sort();

        for w in sorted_deps {
            if !state.indices.contains_key(w.as_str()) {
                strongconnect(w, graph, state);
                let w_lowlink = state.lowlinks[w.as_str()];
                let v_lowlink = state.lowlinks.get_mut(v).unwrap();
                if w_lowlink < *v_lowlink {
                    *v_lowlink = w_lowlink;
                }
            } else if state.on_stack.get(w.as_str()).copied().unwrap_or(false) {
                let w_index = state.indices[w.as_str()];
                let v_lowlink = state.lowlinks.get_mut(v).unwrap();
                if w_index < *v_lowlink {
                    *v_lowlink = w_index;
                }
            }
        }
    }

    // If v is a root node, pop the SCC
    if state.lowlinks[v] == state.indices[v] {
        let mut component = Vec::new();
        loop {
            let w = state.stack.pop().unwrap();
            state.on_stack.insert(w.clone(), false);
            component.push(w.clone());
            if w == v {
                break;
            }
        }
        component.sort();
        state.result.push(component);
    }
}
