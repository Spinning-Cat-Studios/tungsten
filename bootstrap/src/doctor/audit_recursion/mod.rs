//! Recursion audit: identifies and classifies all recursive functions.
//!
//! Operates on the elaborated Core IR terms (no codegen required).
//!
//! 1. Builds a call graph from CoreDef terms
//! 2. Finds strongly connected components (Tarjan's algorithm)
//! 3. Classifies each recursive function's recursion type
//!
//! See ADR 18.4.26g §4 for design rationale.

mod bridge;
mod classify;
#[cfg(test)]
mod classify_tests;
mod decompose_hint;

pub use bridge::{AnalysisMode, CodegenVerdict};
pub use decompose_hint::DecomposeHint;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

use classify::RecursionKind;
use decompose_hint::classify_decompose_hint;
use tungsten_core::terms::termination::{tarjan_scc, Adjacency, OccurrenceGraph};
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

/// Run the recursion audit command (source-only entry — no codegen consult).
///
/// The `#[cfg(not(feature = "codegen"))]` build and `--source-only` reach the
/// audit through here with [`AnalysisMode::SourceOnly`] and no verdicts. The
/// codegen-consulting path is dispatched binary-side (ADR 1.7.26b §2.2) and
/// calls [`cmd_audit_recursion_with_verdicts`].
pub fn cmd_audit_recursion(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    cmd_audit_recursion_with_verdicts(
        file,
        verbose,
        max_errors,
        AnalysisMode::SourceOnly,
        &HashMap::new(),
    )
}

/// Run the recursion audit, downgrading over-optimistic `✓ musttail eligible`
/// verdicts using the actual codegen decisions (ADR 1.7.26b §2.2).
#[allow(clippy::implicit_hasher)] // Reason: callers all use the default hasher
pub fn cmd_audit_recursion_with_verdicts(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    mode: AnalysisMode,
    verdicts: &HashMap<String, CodegenVerdict>,
) -> ExitCode {
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

    // The termination gate's occurrence graph, reused verbatim (ADR 29.6.26e
    // § Findings): the audit and the gate must not disagree about what a
    // recursive group is, and two Tarjans over the same terms is how they would.
    let graph = OccurrenceGraph::build(def_map.iter().map(|(name, term)| (name.as_str(), *term)));

    if verbose {
        eprintln!(
            "Call graph: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    }

    // Find SCCs and classify recursive functions
    let adjacency = graph.adjacency();
    let sccs = tarjan_scc(&adjacency);
    let results = classify_scc_components(&sccs, &adjacency, &def_map, &type_map);

    let recursive_count: usize = results.len();
    print_recursion_report(
        &results,
        project.defs.len(),
        recursive_count,
        &mode,
        verdicts,
    );

    ExitCode::SUCCESS
}

/// Classify all SCC components into recursion kinds.
fn classify_scc_components<'a>(
    sccs: &[Vec<String>],
    adjacency: &Adjacency,
    def_map: &HashMap<String, &'a tungsten_core::terms::Term>,
    type_map: &HashMap<String, &Type>,
) -> Vec<RecursionInfo> {
    let mut results: Vec<RecursionInfo> = Vec::new();

    for component in sccs {
        if component.len() == 1 {
            let name = &component[0];
            if adjacency.get(name).is_some_and(|out| out.contains(name)) {
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
fn print_recursion_report(
    results: &[RecursionInfo],
    total_defs: usize,
    recursive_count: usize,
    mode: &AnalysisMode,
    verdicts: &HashMap<String, CodegenVerdict>,
) {
    println!("Recursion Audit Report");
    println!("══════════════════════");
    println!();
    mode.print_banner();
    println!("Total functions analyzed: {}", total_defs);
    println!("Recursive functions:     {}", recursive_count);
    println!();

    // Only the tail-recursive group's ✓ is downgraded by codegen verdicts —
    // that is the group that claims "musttail eligible ⇒ constant stack".
    print_recursion_group(
        "TAIL-RECURSIVE (musttail eligible):",
        "✓",
        results,
        RecursionKind::TailRecursive,
        verdicts,
    );
    print_recursion_group(
        "TREE-RECURSIVE (stack depth = O(tree height)):",
        "⚠",
        results,
        RecursionKind::TreeRecursive,
        &HashMap::new(),
    );
    print_recursion_group(
        "LINEAR NON-TAIL (stack depth = O(n)):",
        "⚠",
        results,
        RecursionKind::LinearNonTail,
        &HashMap::new(),
    );
    print_recursion_group(
        "GENERAL / UNBOUNDED:",
        "✗",
        results,
        RecursionKind::General,
        &HashMap::new(),
    );

    if results.is_empty() {
        println!("No recursive functions found.");
    }

    println!("Legend: ✓ = protected  ⚠ = at risk  ✗ = needs review");
}

/// Print a group of recursion results filtered by kind.
///
/// When `verdicts` carries a codegen SKIP for a function (ADR 1.7.26b §2.2), its
/// `✓` is downgraded to `✗ SKIP: <reason>  O(N) stack` and the source-level
/// decompose hint is suppressed (the codegen verdict is authoritative).
fn print_recursion_group(
    header: &str,
    symbol: &str,
    results: &[RecursionInfo],
    kind: RecursionKind,
    verdicts: &HashMap<String, CodegenVerdict>,
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
        if let Some((sym, suffix)) = bridge::tail_override(&r.name, verdicts) {
            // Codegen verdict overrides the source-level ✓ + decompose hint.
            println!("  {sym} {}{}{}", r.name, group_str, suffix);
            continue;
        }
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
