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
    pub fn build_adt_only(adt_types: &AdtTypes) -> Self {
        let nodes: HashSet<String> = adt_types.keys().cloned().collect();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut detailed_edges: Vec<TypeEdge> = Vec::new();

        for (name, (_params, constructors)) in adt_types {
            let mut refs = HashSet::new();
            for ctor in constructors {
                let mut collector = EdgeCollector {
                    edge_targets: &nodes,
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

    /// Build a dependency graph from arbitrary (type name, referenced type
    /// expressions) pairs — ADT constructor fields, record field types, and
    /// alias bodies alike.
    ///
    /// Unlike [`TypeGraph::build_adt_only`], the node set is not ADT-only: the
    /// Phase-1d/1e ordering (ADRs 22.7.26c/d) needs alias and record nodes
    /// so that a type referring to an alias/ADT is ordered *after* it and
    /// can inline the already-resolved referent. Cycles that pass through an
    /// alias — invisible to the ADT-only graph — also become visible here.
    ///
    /// `edge_targets` restricts which referenced names create *edges* (every
    /// `type_refs` key is still a node). The Phase-1d/1e caller passes only
    /// the names whose resolution actually inlines into a referrer (ADTs and
    /// aliases): references **to records** stay nominal by design, so a
    /// referrer→record edge cannot constrain resolution order — but it CAN
    /// weld unrelated types into one giant SCC through a nominal back-edge
    /// (e.g. `Item → TypeDef (record) → TypeDefBody → … → Stmt → Item` on
    /// main.tg), defeating the reverse-topological order inside it
    /// (ADR 22.7.26d). Dropping order-irrelevant edges keeps SCCs to genuine
    /// inline cycles only.
    ///
    /// **That restriction is correct for ordering and WRONG for soundness.**
    /// A dropped edge splits an SCC, and an analysis that trusts the group —
    /// strict positivity, say — then false-accepts a cycle running through the
    /// dropped node (ADR 7.8.26e D3). Do not copy this caller's `edge_targets`
    /// into a correctness check; pass the full node set, or use
    /// [`Self::from_adjacency`] with a collector that knows what it needs.
    pub fn build_for_resolution_order(
        type_refs: &[(String, Vec<&Type>)],
        edge_targets: &HashSet<String>,
    ) -> Self {
        let nodes: HashSet<String> = type_refs.iter().map(|(name, _)| name.clone()).collect();
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let mut detailed_edges: Vec<TypeEdge> = Vec::new();

        for (name, referenced) in type_refs {
            let mut refs = HashSet::new();
            let mut collector = EdgeCollector {
                edge_targets,
                refs: &mut refs,
                from_type: name,
                from_ctor: "",
                detailed: &mut detailed_edges,
            };
            for ty in referenced {
                collector.collect_type_refs(ty);
            }
            edges.insert(name.clone(), refs);
        }

        TypeGraph {
            nodes,
            edges,
            detailed_edges,
        }
    }

    /// Build a graph from adjacency computed elsewhere.
    ///
    /// Used by the strict-positivity driver (ADR 7.8.26e D3), whose edge
    /// collector cannot be [`EdgeCollector`]: that one drops `Eq` witness terms
    /// (via `Type::children()`, by documented design) and records edges for
    /// `Forall`/`Mu` binders and type parameters that shadow a definition name.
    /// Both are unacceptable there — a missed edge splits an SCC (false accept)
    /// and a spurious one widens it (false rejection, with no escape hatch).
    /// The collector lives beside the walker in
    /// `tungsten_core::types::positivity` so the "collector traverses a
    /// superset of the walker" invariant is checkable in one place.
    ///
    /// Detailed (per-constructor) edges are not reconstructible from adjacency
    /// alone, so [`Self::edges_between`] returns nothing for such a graph.
    pub fn from_adjacency(adjacency: impl IntoIterator<Item = (String, HashSet<String>)>) -> Self {
        let edges: HashMap<String, HashSet<String>> = adjacency.into_iter().collect();
        TypeGraph {
            nodes: edges.keys().cloned().collect(),
            edges,
            detailed_edges: Vec::new(),
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
    edge_targets: &'a HashSet<String>,
    refs: &'a mut HashSet<String>,
    from_type: &'a str,
    from_ctor: &'a str,
    detailed: &'a mut Vec<TypeEdge>,
}

impl<'a> EdgeCollector<'a> {
    /// Record a type edge if the target is a known type.
    fn record_edge(&mut self, name: &str) {
        if self.edge_targets.contains(name) {
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
    ///
    /// Only the name-bearing variants (`TyVar`, `App`, `Adt`) record an edge;
    /// every other variant contributes no edge of its own and just recurses
    /// into its children via [`Type::children`].
    fn collect_type_refs(&mut self, ty: &Type) {
        match ty {
            Type::TyVar(name) => {
                // Strip @ prefix for named types
                let lookup = name.strip_prefix('@').unwrap_or(name);
                self.record_edge(lookup);
            }

            // Type application: check base name + args
            Type::App(name, args) => {
                self.record_edge(name);
                for arg in args {
                    self.collect_type_refs(arg);
                }
            }

            // Flat ADT: record the name, then recurse args + variant payloads
            Type::Adt(name, type_args, variants) => {
                if self.edge_targets.contains(name) {
                    self.refs.insert(name.to_string());
                }
                for arg in type_args {
                    self.collect_type_refs(arg);
                }
                for (_, vty) in variants {
                    self.collect_type_refs(vty);
                }
            }

            // Every other variant records no edge — recurse structurally.
            _ => {
                for child in ty.children() {
                    self.collect_type_refs(child);
                }
            }
        }
    }
}
