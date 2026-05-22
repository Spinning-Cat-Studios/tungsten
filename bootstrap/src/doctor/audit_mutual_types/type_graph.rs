//! Type dependency graph construction from ADT definitions.
//!
//! Scans each ADT's constructor fields and records type → type edges
//! for any references to other known ADT types.

use std::collections::{HashMap, HashSet};

use crate::driver::AdtTypes;
use tungsten_core::Type;

/// A directed type dependency graph: maps type name → set of referenced types.
pub struct TypeGraph {
    /// All type names (nodes)
    nodes: HashSet<String>,
    /// Adjacency list: type → set of referenced types
    edges: HashMap<String, HashSet<String>>,
    /// Detailed edges: (from_type, from_ctor, to_type) for reporting
    detailed_edges: Vec<TypeEdge>,
}

/// A single edge in the type dependency graph, with constructor context.
#[derive(Debug, Clone)]
pub struct TypeEdge {
    pub from_type: String,
    pub from_ctor: String,
    pub to_type: String,
}

impl TypeGraph {
    /// Build a type dependency graph from ADT type definitions.
    pub fn build(adt_types: &AdtTypes) -> Self {
        let nodes: HashSet<String> = adt_types.keys().cloned().collect();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut detailed_edges: Vec<TypeEdge> = Vec::new();

        for (name, (_params, constructors)) in adt_types {
            let mut refs = HashSet::new();
            for ctor in constructors {
                let mut collector = EdgeCollector {
                    known_types: &nodes,
                    refs: &mut refs,
                    from_type: name,
                    from_ctor: &ctor.name,
                    detailed: &mut detailed_edges,
                };
                for field in &ctor.fields {
                    collector.collect_type_refs(field);
                }
            }
            edges.insert(name.to_string(), refs);
        }

        TypeGraph {
            nodes,
            edges,
            detailed_edges,
        }
    }

    /// Get all nodes (type names).
    pub fn nodes(&self) -> &HashSet<String> {
        &self.nodes
    }

    /// Get the types referenced by a given type.
    pub fn callees(&self, name: &str) -> Option<&HashSet<String>> {
        self.edges.get(name)
    }

    /// Check if there is an edge from `from` to `to`.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges.get(from).map_or(false, |refs| refs.contains(to))
    }

    /// Total number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|s| s.len()).sum()
    }

    /// Get detailed edges between two types (for reporting which constructors create the link).
    pub fn edges_between(&self, from: &str, to: &str) -> Vec<&TypeEdge> {
        self.detailed_edges
            .iter()
            .filter(|e| e.from_type == from && e.to_type == to)
            .collect()
    }
}

/// Collects type edges from constructor fields into the dependency graph.
///
/// Bundles the shared context (known types, ref set, source type/ctor,
/// detailed edge list) to avoid threading 6 parameters through each call.
struct EdgeCollector<'a> {
    known_types: &'a HashSet<String>,
    refs: &'a mut HashSet<String>,
    from_type: &'a str,
    from_ctor: &'a str,
    detailed: &'a mut Vec<TypeEdge>,
}

impl<'a> EdgeCollector<'a> {
    /// Record a type edge if the target is a known type.
    fn record_edge(&mut self, name: &str) {
        if self.known_types.contains(name) {
            if self.refs.insert(name.to_string())
                || !self.detailed.iter().any(|e| {
                    e.from_type == self.from_type
                        && e.from_ctor == self.from_ctor
                        && e.to_type == name
                })
            {
                self.detailed.push(TypeEdge {
                    from_type: self.from_type.to_string(),
                    from_ctor: self.from_ctor.to_string(),
                    to_type: name.to_string(),
                });
            }
        }
    }

    /// Recursively collect type references in a type expression.
    fn collect_type_refs(&mut self, ty: &Type) {
        match ty {
            Type::TyVar(name) => {
                // Strip @ prefix for named types
                let lookup = name.strip_prefix('@').unwrap_or(name);
                self.record_edge(lookup);
            }

            // Binary types
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                self.collect_type_refs(a);
                self.collect_type_refs(b);
            }

            // Binding types
            Type::Forall(_, body) | Type::Mu(_, body) => {
                self.collect_type_refs(body);
            }

            // Type application: check base name + args
            Type::App(name, args) => {
                self.record_edge(name);
                for arg in args {
                    self.collect_type_refs(arg);
                }
            }

            // Flat ADT
            Type::Adt(name, type_args, variants) => {
                if self.known_types.contains(name) {
                    self.refs.insert(name.to_string());
                }
                for arg in type_args {
                    self.collect_type_refs(arg);
                }
                for (_, vty) in variants {
                    self.collect_type_refs(vty);
                }
            }

            // Pointer/ref types
            Type::Ptr(inner) | Type::Ref(inner) => {
                self.collect_type_refs(inner);
            }

            // Eq types
            Type::Eq(ty_arg, _, _) => {
                self.collect_type_refs(ty_arg);
            }

            // Terminal types
            Type::Nat
            | Type::Bool
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => {}
        }
    }
}
