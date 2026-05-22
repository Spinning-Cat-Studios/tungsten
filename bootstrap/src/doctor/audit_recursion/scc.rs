//! Tarjan's algorithm for finding strongly connected components.
//!
//! Used to identify recursive function groups in the call graph.

use std::collections::HashMap;

use super::call_graph::CallGraph;

/// Find all strongly connected components using Tarjan's algorithm.
///
/// Returns components in reverse topological order (leaf SCCs first).
pub fn tarjan_scc(graph: &CallGraph) -> Vec<Vec<String>> {
    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashMap::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        result: Vec::new(),
    };

    let mut nodes: Vec<&String> = graph.nodes().iter().collect();
    nodes.sort(); // Deterministic ordering

    for node in nodes {
        if !state.indices.contains_key(node.as_str()) {
            strongconnect(node, graph, &mut state);
        }
    }

    state.result
}

struct TarjanState {
    index_counter: usize,
    stack: Vec<String>,
    on_stack: HashMap<String, bool>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    result: Vec<Vec<String>>,
}

fn strongconnect(v: &str, graph: &CallGraph, state: &mut TarjanState) {
    let v_index = state.index_counter;
    state.index_counter += 1;
    state.indices.insert(v.to_string(), v_index);
    state.lowlinks.insert(v.to_string(), v_index);
    state.stack.push(v.to_string());
    state.on_stack.insert(v.to_string(), true);

    // Consider successors of v
    if let Some(callees) = graph.callees(v) {
        let mut sorted_callees: Vec<&String> = callees.iter().collect();
        sorted_callees.sort();

        for w in sorted_callees {
            if !state.indices.contains_key(w.as_str()) {
                // Successor w has not yet been visited; recurse
                strongconnect(w, graph, state);
                let w_lowlink = state.lowlinks[w.as_str()];
                let v_lowlink = state.lowlinks.get_mut(v).unwrap();
                if w_lowlink < *v_lowlink {
                    *v_lowlink = w_lowlink;
                }
            } else if state.on_stack.get(w.as_str()).copied().unwrap_or(false) {
                // Successor w is on stack → part of current SCC
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
        component.sort(); // Deterministic ordering within SCC
        state.result.push(component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tungsten_core::terms::Term;
    use tungsten_core::types::Type;

    #[test]
    fn test_no_cycles() {
        let a = Term::NatLit(1);
        let b = Term::App(
            Box::new(Term::Global("a".to_string())),
            Box::new(Term::Unit),
        );
        let mut defs = HashMap::new();
        defs.insert("a".to_string(), &a);
        defs.insert("b".to_string(), &b);

        let graph = CallGraph::build(&defs);
        let sccs = tarjan_scc(&graph);

        // All SCCs should be singletons
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }

    #[test]
    fn test_self_loop() {
        let f = Term::App(
            Box::new(Term::Global("f".to_string())),
            Box::new(Term::Unit),
        );
        let mut defs = HashMap::new();
        defs.insert("f".to_string(), &f);

        let graph = CallGraph::build(&defs);
        let sccs = tarjan_scc(&graph);

        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0], vec!["f".to_string()]);
    }

    #[test]
    fn test_mutual_recursion() {
        // f calls g, g calls f
        let f = Term::App(
            Box::new(Term::Global("g".to_string())),
            Box::new(Term::Unit),
        );
        let g = Term::App(
            Box::new(Term::Global("f".to_string())),
            Box::new(Term::Unit),
        );
        let mut defs = HashMap::new();
        defs.insert("f".to_string(), &f);
        defs.insert("g".to_string(), &g);

        let graph = CallGraph::build(&defs);
        let sccs = tarjan_scc(&graph);

        // Should find one SCC with both f and g
        let mutual: Vec<_> = sccs.iter().filter(|s| s.len() > 1).collect();
        assert_eq!(mutual.len(), 1);
        assert!(mutual[0].contains(&"f".to_string()));
        assert!(mutual[0].contains(&"g".to_string()));
    }
}
