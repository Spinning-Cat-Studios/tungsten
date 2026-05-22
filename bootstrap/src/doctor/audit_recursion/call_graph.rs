//! Call graph construction from Core IR terms.
//!
//! Walks each definition's term tree and records caller → callee edges
//! for Global references (calls to other top-level definitions).

use std::collections::{HashMap, HashSet};
use tungsten_core::terms::Term;

/// A directed call graph: maps function name → set of called function names.
pub struct CallGraph {
    /// All function names (nodes)
    nodes: HashSet<String>,
    /// Adjacency list: caller → set of callees
    edges: HashMap<String, HashSet<String>>,
}

impl CallGraph {
    /// Build a call graph from a map of definition names to their terms.
    pub fn build(defs: &HashMap<String, &Term>) -> Self {
        let nodes: HashSet<String> = defs.keys().cloned().collect();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();

        for (name, term) in defs {
            let mut callees = HashSet::new();
            collect_calls(term, &nodes, &mut callees);
            edges.insert(name.clone(), callees);
        }

        CallGraph { nodes, edges }
    }

    /// Get all nodes (function names).
    pub fn nodes(&self) -> &HashSet<String> {
        &self.nodes
    }

    /// Get the callees of a function.
    pub fn callees(&self, name: &str) -> Option<&HashSet<String>> {
        self.edges.get(name)
    }

    /// Check if there is an edge from `from` to `to`.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges
            .get(from)
            .map_or(false, |callees| callees.contains(to))
    }

    /// Total number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|s| s.len()).sum()
    }
}

/// Recursively collect all Global references in a term that refer to known definitions.
fn collect_calls(term: &Term, known_defs: &HashSet<String>, callees: &mut HashSet<String>) {
    if let Term::Global(name) = term {
        if known_defs.contains(name) {
            callees.insert(name.clone());
        }
    }
    term.for_each_subterm(|child| collect_calls(child, known_defs, callees));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::types::Type;

    #[test]
    fn test_self_recursive_function() {
        // f = fix f. λx. f x
        let term = Term::Fix(
            "f".to_string(),
            Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
            Box::new(Term::Lambda(
                "x".to_string(),
                Type::Nat,
                Box::new(Term::App(
                    Box::new(Term::Global("f".to_string())),
                    Box::new(Term::Var("x".to_string())),
                )),
            )),
        );

        let mut defs = HashMap::new();
        defs.insert("f".to_string(), &term);
        let graph = CallGraph::build(&defs);

        assert!(graph.has_edge("f", "f"));
    }

    #[test]
    fn test_no_recursion() {
        let term_a = Term::NatLit(42);
        let term_b = Term::App(
            Box::new(Term::Global("a".to_string())),
            Box::new(Term::Unit),
        );

        let mut defs = HashMap::new();
        defs.insert("a".to_string(), &term_a);
        defs.insert("b".to_string(), &term_b);
        let graph = CallGraph::build(&defs);

        assert!(!graph.has_edge("a", "a"));
        assert!(!graph.has_edge("b", "b"));
        assert!(graph.has_edge("b", "a"));
    }
}
