//! Recursion audit: identifies and classifies all recursive functions.
//!
//! Operates on the elaborated Core IR terms (no codegen required).
//!
//! 1. Builds a call graph from CoreDef terms
//! 2. Finds strongly connected components (Tarjan's algorithm)
//! 3. Classifies each recursive function's recursion type
//!
//! See ADR 18.4.26g §4 for design rationale.

mod call_graph;
mod classify;
#[cfg(test)]
mod classify_tests;
mod scc;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

use call_graph::CallGraph;
use classify::RecursionKind;
use scc::tarjan_scc;
use tungsten_core::types::Type;

/// Result of analyzing a single function's recursion.
#[derive(Debug)]
pub struct RecursionInfo {
    /// The function name
    pub name: String,
    /// The kind of recursion detected
    pub kind: RecursionKind,
    /// Names of other functions in the same recursive group (if mutually recursive)
    pub group: Vec<String>,
    /// Decomposition hint for musttail (ADR 18.5.26a)
    pub decompose_hint: DecomposeHint,
}

/// Run the recursion audit command.
pub fn cmd_audit_recursion(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    // Elaborate the project
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Build call graph and type map
    let mut def_map: HashMap<String, &tungsten_core::terms::Term> = HashMap::new();
    let mut type_map: HashMap<String, &Type> = HashMap::new();
    for def in &project.defs {
        def_map.insert(def.name.clone(), &def.term.term);
        type_map.insert(def.name.clone(), &def.ty);
    }

    let graph = CallGraph::build(&def_map);

    if verbose {
        eprintln!(
            "Call graph: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    }

    // Find SCCs and classify recursive functions
    let sccs = tarjan_scc(&graph);
    let results = classify_scc_components(&sccs, &graph, &def_map, &type_map);

    let recursive_count: usize = results.len();
    print_recursion_report(&results, project.defs.len(), recursive_count);

    ExitCode::SUCCESS
}

/// Classify all SCC components into recursion kinds.
fn classify_scc_components<'a>(
    sccs: &[Vec<String>],
    graph: &CallGraph,
    def_map: &HashMap<String, &'a tungsten_core::terms::Term>,
    type_map: &HashMap<String, &Type>,
) -> Vec<RecursionInfo> {
    let mut results: Vec<RecursionInfo> = Vec::new();

    for component in sccs {
        if component.len() == 1 {
            let name = &component[0];
            if graph.has_edge(name, name) {
                let kind = classify_single(name, def_map);
                let decompose_hint = if kind == RecursionKind::TailRecursive {
                    classify_decompose_hint(name, type_map)
                } else {
                    DecomposeHint::NotTailRecursive
                };
                results.push(RecursionInfo {
                    name: name.clone(),
                    kind,
                    group: vec![],
                    decompose_hint,
                });
            }
        } else {
            for name in component {
                let kind = classify_single(name, def_map);
                let group: Vec<String> = component.iter().filter(|n| *n != name).cloned().collect();
                results.push(RecursionInfo {
                    name: name.clone(),
                    kind,
                    group,
                    decompose_hint: DecomposeHint::NotTailRecursive,
                });
            }
        }
    }

    results
}

/// Classify a single function's recursion kind.
fn classify_single(
    name: &str,
    def_map: &HashMap<String, &tungsten_core::terms::Term>,
) -> RecursionKind {
    if let Some(term) = def_map.get(name) {
        classify::classify_recursion(name, term)
    } else {
        RecursionKind::General
    }
}

/// Print the recursion audit report.
fn print_recursion_report(results: &[RecursionInfo], total_defs: usize, recursive_count: usize) {
    println!("Recursion Audit Report");
    println!("══════════════════════");
    println!();
    println!("Total functions analyzed: {}", total_defs);
    println!("Recursive functions:     {}", recursive_count);
    println!();

    print_recursion_group(
        "TAIL-RECURSIVE (musttail eligible):",
        "✓",
        results,
        RecursionKind::TailRecursive,
    );
    print_recursion_group(
        "TREE-RECURSIVE (stack depth = O(tree height)):",
        "⚠",
        results,
        RecursionKind::TreeRecursive,
    );
    print_recursion_group(
        "LINEAR NON-TAIL (stack depth = O(n)):",
        "⚠",
        results,
        RecursionKind::LinearNonTail,
    );
    print_recursion_group("GENERAL / UNBOUNDED:", "✗", results, RecursionKind::General);

    if results.is_empty() {
        println!("No recursive functions found.");
    }

    println!("Legend: ✓ = protected  ⚠ = at risk  ✗ = needs review");
}

/// Print a group of recursion results filtered by kind.
fn print_recursion_group(
    header: &str,
    symbol: &str,
    results: &[RecursionInfo],
    kind: RecursionKind,
) {
    let filtered: Vec<_> = results.iter().filter(|r| r.kind == kind).collect();
    if filtered.is_empty() {
        return;
    }
    println!("{header}");
    for r in &filtered {
        let group_str = if r.group.is_empty() {
            String::new()
        } else {
            format!(" [mutual: {}]", r.group.join(", "))
        };
        let hint_str = match &r.decompose_hint {
            DecomposeHint::NoStructParams => "",
            DecomposeHint::Eligible(n) => {
                if *n == 1 {
                    " [1 struct param → decompose]"
                } else {
                    " [struct params → decompose]"
                }
            }
            DecomposeHint::Ineligible(reason) => reason,
            DecomposeHint::NotTailRecursive => "",
        };
        println!("  {symbol} {}{}{}", r.name, group_str, hint_str);
    }
    println!();
}

/// Decomposition eligibility hint for musttail (ADR 18.5.26a).
#[derive(Debug)]
pub enum DecomposeHint {
    /// No struct-typed params — musttail works directly.
    NoStructParams,
    /// Has struct params that are decomposition-eligible. Count of struct params.
    Eligible(usize),
    /// Has struct params but not eligible for decomposition.
    Ineligible(&'static str),
    /// Not tail-recursive, so decomposition is irrelevant.
    NotTailRecursive,
}

/// Classify a tail-recursive function's decomposition eligibility based on Core IR types.
fn classify_decompose_hint(name: &str, type_map: &HashMap<String, &Type>) -> DecomposeHint {
    let ty = match type_map.get(name) {
        Some(t) => t,
        None => return DecomposeHint::NoStructParams,
    };
    let params = collect_param_types(ty);
    if params.is_empty() {
        return DecomposeHint::NoStructParams;
    }
    let mut struct_count = 0;
    for param in &params {
        match param {
            // String lowers to { ptr, i64 } — flattenable
            Type::String => struct_count += 1,
            // Product lowers to struct — flattenable if fields are scalar
            Type::Product(_, _) => struct_count += 1,
            // Sum/Arrow lower to structs with nested struct/array fields
            Type::Sum(_, _) | Type::Arrow(_, _) => {
                return DecomposeHint::Ineligible(" [struct params, not flattenable]");
            }
            _ => {} // scalar or pointer — no struct issue
        }
    }
    if struct_count > 0 {
        DecomposeHint::Eligible(struct_count)
    } else {
        DecomposeHint::NoStructParams
    }
}

/// Extract parameter types from a function type (peeling Arrow wrappers).
fn collect_param_types(ty: &Type) -> Vec<Type> {
    let mut params = Vec::new();
    let mut current = ty;
    while let Type::Arrow(param, ret) = current {
        params.push((**param).clone());
        current = ret;
    }
    // Also handle Forall wrappers (polymorphic functions)
    if let Type::Forall(_, body) = ty {
        return collect_param_types(body);
    }
    params
}

#[cfg(test)]
mod decompose_hint_tests {
    use super::*;

    fn make_type_map(entries: Vec<(&str, Type)>) -> HashMap<String, Type> {
        entries
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .collect()
    }

    fn classify_with(name: &str, ty: Type) -> DecomposeHint {
        let map = make_type_map(vec![(name, ty)]);
        let ref_map: HashMap<String, &Type> = map.iter().map(|(k, v)| (k.clone(), v)).collect();
        classify_decompose_hint(name, &ref_map)
    }

    #[test]
    fn scalar_params_no_decompose() {
        // fn(Nat) -> Nat
        let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::NoStructParams
        ));
    }

    #[test]
    fn string_param_is_eligible() {
        // fn(String) -> Nat
        let ty = Type::Arrow(Box::new(Type::String), Box::new(Type::Nat));
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(1)));
    }

    #[test]
    fn product_param_is_eligible() {
        // fn(Product) -> Nat
        let ty = Type::Arrow(
            Box::new(Type::Product(Box::new(Type::Nat), Box::new(Type::Nat))),
            Box::new(Type::Nat),
        );
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(1)));
    }

    #[test]
    fn sum_param_is_ineligible() {
        // fn(Sum) -> Nat — Sum lowers to struct+tag, not flattenable
        let ty = Type::Arrow(
            Box::new(Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool))),
            Box::new(Type::Nat),
        );
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::Ineligible(_)
        ));
    }

    #[test]
    fn arrow_param_is_ineligible() {
        // fn(fn(Nat)->Nat) -> Nat — Arrow lowers to closure struct
        let inner = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
        let ty = Type::Arrow(Box::new(inner), Box::new(Type::Nat));
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::Ineligible(_)
        ));
    }

    #[test]
    fn mixed_string_and_nat() {
        // fn(String, Nat, String) -> Nat — 2 struct params
        let ty = Type::Arrow(
            Box::new(Type::String),
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(Type::Arrow(Box::new(Type::String), Box::new(Type::Nat))),
            )),
        );
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(2)));
    }

    #[test]
    fn unknown_function_returns_no_struct() {
        let map: HashMap<String, Type> = HashMap::new();
        let ref_map: HashMap<String, &Type> = map.iter().map(|(k, v)| (k.clone(), v)).collect();
        assert!(matches!(
            classify_decompose_hint("nonexistent", &ref_map),
            DecomposeHint::NoStructParams
        ));
    }
}
