//! Mutual type recursion audit: identifies mutually recursive type groups.
//!
//! Operates on the elaborated ADT type definitions (no codegen required).
//!
//! 1. Builds a type dependency graph from ADT constructor fields
//! 2. Finds strongly connected components (Tarjan's algorithm)
//! 3. Reports mutually recursive groups, self-recursive types, and leaf types
//!
//! See ADR 18.4.26h §4 for design rationale.

pub mod scc;
pub mod type_graph;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use crate::driver;

use type_graph::TypeGraph;

/// The deterministic, dependency-respecting encode/resolution order shared by
/// the elaborator's Deferred-TyVar Resolution (`resolve_deferred_type_references`) and Encoding Finalization
/// (`cache_type_encodings`) and by `info type encode-order` (ADRs 22.7.26c/d).
///
/// Returns SCCs in reverse-topological order (referents before referrers),
/// each SCC's members sorted lexicographically. Flatten for the linear pass
/// order; keep grouped to see which types share a cycle (a multi-member SCC
/// cannot be internally ordered by topology, so its members resolve in
/// lexicographic order — the fact `info type encode-order` surfaces).
///
/// `edge_targets` restricts which referenced names create *edges*: pass the
/// inline-relevant targets (ADTs + aliases) only. References to records are
/// nominal and must not create edges, or they weld unrelated types into one
/// order-defeating SCC (ADR 22.7.26d). Every `type_refs` key is still a node.
///
/// Keeping this the single kernel behind both the live elaborator and the
/// diagnostic means the tool cannot report an order the compiler does not use.
#[allow(clippy::implicit_hasher)] // Reason: callers all use the default hasher
pub fn encode_order_sccs(
    type_refs: &[(String, Vec<&Type>)],
    edge_targets: &HashSet<String>,
) -> Vec<Vec<String>> {
    let graph = TypeGraph::build_for_resolution_order(type_refs, edge_targets);
    scc::tarjan_scc(&graph)
}

/// Run the mutual type recursion audit command.
pub fn cmd_audit_mutual_types(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    json: bool,
) -> ExitCode {
    // Elaborate the project
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Build type dependency graph
    let graph = TypeGraph::build_adt_only(&project.adt_types);

    if verbose {
        eprintln!(
            "Type graph: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    }

    // Find SCCs
    let sccs = scc::tarjan_scc(&graph);

    // Classify SCCs
    let mut mutual_groups: Vec<Vec<String>> = Vec::new();
    let mut self_recursive: Vec<String> = Vec::new();
    let mut non_recursive: Vec<String> = Vec::new();

    for component in &sccs {
        if component.len() == 1 {
            let name = &component[0];
            if graph.has_edge(name, name) {
                self_recursive.push(name.clone());
            } else {
                non_recursive.push(name.clone());
            }
        } else {
            mutual_groups.push(component.clone());
        }
    }

    if json {
        print_json_report(&mutual_groups, &self_recursive, &non_recursive, &graph);
    } else {
        print_text_report(&mutual_groups, &self_recursive, &non_recursive, &graph);
    }

    ExitCode::SUCCESS
}

fn print_text_report(
    mutual_groups: &[Vec<String>],
    self_recursive: &[String],
    non_recursive: &[String],
    graph: &TypeGraph,
) {
    println!("Mutual Type Recursion Audit");
    println!("══════════════════════════");
    println!();

    if !mutual_groups.is_empty() {
        println!(
            "Mutual Recursion Groups (SCCs with >1 member): {}",
            mutual_groups.len()
        );
        println!();

        for (i, group) in mutual_groups.iter().enumerate() {
            println!("  Group {} ({} types):", i + 1, group.len());
            println!("    {}", group.join(" ↔ "));

            // Show cross-type edges
            for from in group {
                for to in group {
                    if from == to {
                        continue;
                    }
                    for edge in graph.edges_between(from, to) {
                        println!(
                            "    {}.{} → {}",
                            edge.from_type, edge.from_ctor, edge.to_type
                        );
                    }
                }
            }
            println!();
        }
    } else {
        println!("No mutually recursive type groups found.");
        println!();
    }

    if !self_recursive.is_empty() {
        println!("Self-Recursive Types: {}", self_recursive.len());
        for name in self_recursive {
            // Show which constructors create the self-reference
            let self_edges = graph.edges_between(name, name);
            if self_edges.is_empty() {
                println!("  {name}");
            } else {
                let ctors: Vec<&str> = self_edges.iter().map(|e| e.from_ctor.as_str()).collect();
                println!("  {name} ({})", ctors.join(", "));
            }
        }
        println!();
    }

    println!("Non-Recursive Types: {}", non_recursive.len());
}

fn print_json_report(
    mutual_groups: &[Vec<String>],
    self_recursive: &[String],
    non_recursive: &[String],
    graph: &TypeGraph,
) {
    // Build JSON manually to avoid serde dependency
    print!("{{");
    print!("\"mutual_groups\":[");
    for (i, group) in mutual_groups.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("{{\"types\":[");
        for (j, name) in group.iter().enumerate() {
            if j > 0 {
                print!(",");
            }
            print!("\"{}\"", json_escape(name));
        }
        print!("],\"edges\":[");
        let mut first_edge = true;
        for from in group {
            for to in group {
                if from == to {
                    continue;
                }
                for edge in graph.edges_between(from, to) {
                    if !first_edge {
                        print!(",");
                    }
                    first_edge = false;
                    print!(
                        "{{\"from\":\"{}\",\"ctor\":\"{}\",\"to\":\"{}\"}}",
                        json_escape(&edge.from_type),
                        json_escape(&edge.from_ctor),
                        json_escape(&edge.to_type)
                    );
                }
            }
        }
        print!("]}}");
    }
    print!("],\"self_recursive\":[");
    for (i, name) in self_recursive.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("\"{}\"", json_escape(name));
    }
    print!("],\"non_recursive_count\":{}", non_recursive.len());
    println!("}}");
}

/// Minimal JSON string escaping.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
