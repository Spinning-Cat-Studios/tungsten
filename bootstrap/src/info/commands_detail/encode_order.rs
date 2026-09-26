//! `info type encode-order` — the deterministic Phase-1e encode order
//! (ADR 22.7.26d).
//!
//! Answers "why does type X resolve/encode before type Y?" — the question
//! ADR 22.7.26d had to reconstruct by hand with throwaway instrumentation.
//! Prints the reverse-topological order (referents before referrers) with each
//! type's position, and highlights **multi-member SCCs**, where topology cannot
//! order the members so they fall back to lexicographic order — the class where
//! a record could resolve before its own referents and freeze `@`-deferred
//! references (the nominal-back-edge weld the ADR fixed by dropping
//! referrer→record edges). On healthy code every SCC is a singleton or a
//! genuine μ-recursion cycle; a *new* multi-member SCC that mixes a record with
//! its referents is the regression this tool makes visible.
//!
//! The order is produced by the SAME `encode_order_sccs` kernel the elaborator's
//! `dependency_respecting_type_order` uses, over the project's final (resolved,
//! Phase-1e) type bodies — so it faithfully reflects the compiler's Phase-1e
//! encode order. It does NOT reconstruct the Phase-1d *deferred*-reference graph
//! (where `@`-references are still unresolved and the freeze physically occurs);
//! for that live view set `TUNGSTEN_TRACE_ENCODING=<type>` and re-run any
//! elaboration (ADR 22.7.26d).

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::doctor::audit_mutual_types::encode_order_sccs;
use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_core::Type;

use crate::info::elaborate_for_info;

/// Show the deterministic Phase-1d/1e resolution/encode order for a project.
///
/// `focus` narrows the report to one type: its position, its SCC, and the
/// positions of the inline-relevant types it references (a referent resolving
/// *after* it is the freeze hazard). `show_all` dumps the full linear order.
pub fn cmd_info_encode_order(
    file: &PathBuf,
    focus: Option<&str>,
    show_all: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let type_refs = collect_type_refs(&project);
    let edge_targets = inline_relevant_targets(&project);
    let sccs = encode_order_sccs(&type_refs, &edge_targets);

    // Linear order + each type's position (index in the flattened emission).
    let order: Vec<&String> = sccs.iter().flatten().collect();
    let position: std::collections::HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();

    if let Some(target) = focus {
        return report_focus(target, &position, &sccs, &edge_targets, &type_refs);
    }

    report_summary(&order, &sccs, show_all);
    ExitCode::SUCCESS
}

/// Build the `(name, referenced types)` pairs for every non-stub type, matching
/// `dependency_respecting_type_order`'s assembly from the elaborator env.
fn collect_type_refs(project: &ProjectOutput) -> Vec<(String, Vec<&Type>)> {
    let mut refs: Vec<(String, Vec<&Type>)> = Vec::new();
    for (name, (_params, ctors)) in &project.adt_types {
        refs.push((
            name.clone(),
            ctors.iter().flat_map(|c| c.fields.iter()).collect(),
        ));
    }
    for (name, fields) in &project.record_types {
        refs.push((name.clone(), fields.iter().map(|(_, ty)| ty).collect()));
    }
    for (name, (_params, body)) in &project.type_aliases {
        refs.push((name.clone(), vec![body]));
    }
    refs
}

/// The inline-relevant edge targets: ADTs + aliases (records are nominal and
/// must not create edges — ADR 22.7.26d).
fn inline_relevant_targets(project: &ProjectOutput) -> HashSet<String> {
    project
        .adt_types
        .keys()
        .chain(project.type_aliases.keys())
        .cloned()
        .collect()
}

/// Print the whole-project summary: counts, the multi-member SCCs (where the
/// order is decided lexically), and optionally the full linear order.
fn report_summary(order: &[&String], sccs: &[Vec<String>], show_all: bool) {
    let multi: Vec<&Vec<String>> = sccs.iter().filter(|s| s.len() > 1).collect();
    let singletons = sccs.len() - multi.len();

    println!("Encode / resolution order (ADR 22.7.26d)");
    println!("════════════════════════════════════════");
    println!(
        "{} type(s): {singletons} singleton SCC(s) + {} multi-member SCC(s).",
        order.len(),
        multi.len()
    );
    println!("Reverse-topological (referents before referrers); within a");
    println!("multi-member SCC, members resolve in lexicographic order.");
    println!();

    if multi.is_empty() {
        println!("No multi-member SCCs — every type resolves after all it references.");
    } else {
        println!("Multi-member SCCs (order decided lexically, not by dependency):");
        // sccs is the reverse-topo emission; a member's position is its flat index.
        let mut flat = 0usize;
        for scc in sccs {
            if scc.len() > 1 {
                println!("  {}-member cycle:", scc.len());
                for (offset, name) in scc.iter().enumerate() {
                    println!("    {:>4}  {name}", flat + offset);
                }
            }
            flat += scc.len();
        }
    }

    if show_all {
        println!();
        println!("Full linear order:");
        for (i, name) in order.iter().enumerate() {
            println!("  {i:>4}  {name}");
        }
    }
}

/// Print the focus report for one type: its position, its SCC, and where the
/// inline-relevant types it references land (a referent *after* it is frozen).
fn report_focus(
    target: &str,
    position: &std::collections::HashMap<&str, usize>,
    sccs: &[Vec<String>],
    edge_targets: &HashSet<String>,
    type_refs: &[(String, Vec<&Type>)],
) -> ExitCode {
    let Some(&pos) = position.get(target) else {
        eprintln!("type not found in encode order: {target}");
        return ExitCode::FAILURE;
    };

    println!("{target}: resolves at position {pos} of {}", position.len());

    if let Some(scc) = sccs.iter().find(|s| s.iter().any(|n| n == target)) {
        if scc.len() > 1 {
            println!(
                "  in a {}-member SCC (within-SCC order is lexicographic): {{{}}}",
                scc.len(),
                scc.join(", ")
            );
        } else {
            println!("  singleton SCC (not in a cycle)");
        }
    }

    let referents = referenced_edge_targets(target, edge_targets, type_refs);
    if referents.is_empty() {
        println!("  references no inline-relevant (ADT/alias) types");
    } else {
        println!("  references (inline-relevant); a referent AFTER this position freezes:");
        for name in referents {
            match position.get(name.as_str()) {
                Some(&rp) if rp > pos => println!("    {rp:>4}  {name}  ⚠ resolves after"),
                Some(&rp) => println!("    {rp:>4}  {name}"),
                None => println!("       ?  {name}  (not ordered)"),
            }
        }
    }
    ExitCode::SUCCESS
}

/// The inline-relevant named types `target` references (deduped, sorted).
fn referenced_edge_targets(
    target: &str,
    edge_targets: &HashSet<String>,
    type_refs: &[(String, Vec<&Type>)],
) -> Vec<String> {
    let mut names: HashSet<String> = HashSet::new();
    if let Some((_, refs)) = type_refs.iter().find(|(name, _)| name == target) {
        for ty in refs {
            collect_named_refs(ty, edge_targets, &mut names);
        }
    }
    let mut out: Vec<String> = names.into_iter().collect();
    out.sort();
    out
}

/// Collect names of `edge_targets` types referenced anywhere in `ty` (strips the
/// `@` deferred-reference prefix, matching the edge collector).
fn collect_named_refs(ty: &Type, edge_targets: &HashSet<String>, out: &mut HashSet<String>) {
    match ty {
        Type::TyVar(name) => {
            let bare = name.strip_prefix('@').unwrap_or(name);
            if edge_targets.contains(bare) {
                out.insert(bare.to_string());
            }
        }
        Type::App(name, args) => {
            if edge_targets.contains(name) {
                out.insert(name.clone());
            }
            for arg in args {
                collect_named_refs(arg, edge_targets, out);
            }
        }
        _ => {
            for child in ty.children() {
                collect_named_refs(child, edge_targets, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_file(source: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, source).unwrap();
        (dir, path)
    }

    #[test]
    fn summary_succeeds_on_simple_types() {
        // A record referencing an ADT referencing another ADT — the shape whose
        // ordering ADR 22.7.26d stabilized.
        let (_dir, path) = make_temp_file(
            "type Payload = Wrap(Nat)\n\
             type Embedded = Mk(Payload)\n\
             type Holder = { field: Embedded }\n\
             fn main() -> Nat { 0 }",
        );
        assert_eq!(
            cmd_info_encode_order(&path, None, true, false, 20),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn focus_reports_referent_positions() {
        let (_dir, path) = make_temp_file(
            "type Payload = Wrap(Nat)\n\
             type Embedded = Mk(Payload)\n\
             fn main() -> Nat { 0 }",
        );
        // Embedded references Payload; reverse-topo puts Payload first, so the
        // referent resolves BEFORE Embedded (no freeze). Command must succeed.
        assert_eq!(
            cmd_info_encode_order(&path, Some("Embedded"), false, false, 20),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn focus_on_missing_type_fails() {
        let (_dir, path) = make_temp_file("type A = MkA(Nat)\nfn main() -> Nat { 0 }");
        assert_eq!(
            cmd_info_encode_order(&path, Some("Nonexistent"), false, false, 20),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn referenced_edge_targets_strips_at_prefix_and_filters() {
        let mut targets = HashSet::new();
        targets.insert("Foo".to_string());
        // "Bar" deliberately absent — a nominal record referent is not an edge.
        let foo_ref = Type::TyVar("@Foo".to_string());
        let bar_ref = Type::TyVar("@Bar".to_string());
        let type_refs = vec![("Root".to_string(), vec![&foo_ref, &bar_ref])];
        let got = referenced_edge_targets("Root", &targets, &type_refs);
        assert_eq!(got, vec!["Foo".to_string()]);
    }
}
