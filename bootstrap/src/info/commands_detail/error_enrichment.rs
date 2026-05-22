//! `tungsten info error-enrichment`: show cross-file diagnostic enrichment points.
//!
//! Elaborates a project and reports which function calls would receive cross-file
//! diagnostic notes (ADR 15.5.26a) when type errors occur. Shows both outgoing
//! (calls to other modules) and incoming (public functions callable from elsewhere)
//! enrichment points.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_type_short;
use tungsten_core::terms::Term;

use tungsten_bootstrap::driver::ProjectOutput;

/// Collect all `Term::Global(name)` references from a term, recursively.
fn collect_global_refs(term: &Term) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_global_refs_inner(term, &mut refs);
    refs
}

fn collect_global_refs_inner(term: &Term, refs: &mut BTreeSet<String>) {
    if let Term::Global(name) = term {
        refs.insert(name.clone());
    }
    term.for_each_subterm(|child| collect_global_refs_inner(child, refs));
}

/// Info about a definition's location in the module tree.
struct DefLocation {
    module_path: Vec<String>,
    source_file: PathBuf,
}

struct OutgoingCall {
    name: String,
    module: String,
    file: PathBuf,
    ty: String,
}

/// Build `def_name` → `DefLocation` map from codegen units.
fn build_def_locations(project: &ProjectOutput) -> HashMap<String, DefLocation> {
    let mut map = HashMap::new();
    for unit in &project.codegen_units {
        for def in &unit.defs {
            map.insert(
                def.name.clone(),
                DefLocation {
                    module_path: unit.module_path.clone(),
                    source_file: unit.source_file.clone(),
                },
            );
        }
    }
    map
}

/// Collect incoming enrichment points: public functions defined in target file.
fn collect_incoming<'a>(
    project: &'a ProjectOutput,
    target_defs: &[&str],
) -> Vec<(&'a str, String)> {
    let mut incoming: Vec<(&str, String)> = Vec::new();
    for def in &project.defs {
        if target_defs.contains(&def.name.as_str()) {
            incoming.push((&def.name, format_type_short(&def.ty)));
        }
    }
    incoming.sort_by_key(|(name, _)| *name);
    incoming
}

/// Collect outgoing enrichment points: cross-module calls from target file's defs.
fn collect_outgoing<'a>(
    project: &'a ProjectOutput,
    target_defs: &[&str],
    target_file: &Path,
    def_locations: &HashMap<String, DefLocation>,
) -> BTreeMap<&'a str, Vec<OutgoingCall>> {
    let mut outgoing: BTreeMap<&str, Vec<OutgoingCall>> = BTreeMap::new();

    for def in &project.defs {
        if !target_defs.contains(&def.name.as_str()) {
            continue;
        }
        let refs = collect_global_refs(&def.term.term);
        let mut cross_module_calls = Vec::new();

        for ref_name in &refs {
            if let Some(loc) = def_locations.get(ref_name.as_str()) {
                if normalize_path(&loc.source_file) != target_file {
                    let ref_ty = project
                        .defs
                        .iter()
                        .find(|d| d.name == *ref_name)
                        .map_or_else(|| "?".to_string(), |d| format_type_short(&d.ty));
                    cross_module_calls.push(OutgoingCall {
                        name: ref_name.clone(),
                        module: loc.module_path.join("::"),
                        file: loc.source_file.clone(),
                        ty: ref_ty,
                    });
                }
            }
        }

        if !cross_module_calls.is_empty() {
            outgoing.insert(&def.name, cross_module_calls);
        }
    }
    outgoing
}

/// Print the incoming enrichment section.
fn print_incoming(incoming: &[(&str, String)]) {
    println!("Incoming enrichment (definitions callable from other modules):");
    println!("──────────────────────────────────────────────────────────────");
    if incoming.is_empty() {
        println!("  (none)");
    } else {
        println!("  These functions generate cross-file notes when callers in other");
        println!("  modules encounter type mismatches on arguments or return types.");
        println!();
        for (name, ty) in incoming {
            println!("  {name}");
            println!("    type: {ty}");
            println!("    enrichment: argument mismatch → \"parameter type declared in `{name}`\"");
            println!(
                "    enrichment: return mismatch  → \"return type declared in `{name}`\" + trace"
            );
        }
    }
    println!();
}

/// Print the outgoing enrichment section.
fn print_outgoing(outgoing: &BTreeMap<&str, Vec<OutgoingCall>>) {
    println!("Outgoing enrichment (cross-module calls from this file):");
    println!("────────────────────────────────────────────────────────");
    if outgoing.is_empty() {
        println!("  (none — all calls are intra-module)");
    } else {
        println!("  Type errors on these calls would show cross-file notes");
        println!("  pointing to the callee's definition.");
        println!();
        for (caller, calls) in outgoing {
            println!("  in {caller}:");
            for call in calls {
                let module = if call.module.is_empty() {
                    "(root)"
                } else {
                    &call.module
                };
                println!(
                    "    → {} (module: {}, file: {})",
                    call.name,
                    module,
                    call.file.display()
                );
                println!("      type: {}", call.ty);
            }
        }
    }
    println!();
}

/// Run the error-enrichment info command.
pub fn cmd_info_error_enrichment(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let def_locations = build_def_locations(&project);
    let target_file = normalize_path(file);

    let target_defs: Vec<&str> = def_locations
        .iter()
        .filter(|(_, loc)| normalize_path(&loc.source_file) == target_file)
        .map(|(name, _)| name.as_str())
        .collect();

    if target_defs.is_empty() {
        eprintln!("No definitions found in {}", file.display());
        return ExitCode::FAILURE;
    }

    let target_module = def_locations
        .iter()
        .find(|(_, loc)| normalize_path(&loc.source_file) == target_file)
        .map(|(_, loc)| loc.module_path.clone())
        .unwrap_or_default();

    println!("Error Enrichment Report");
    println!("{}", "═".repeat(23));
    println!();
    println!("File:   {}", file.display());
    let module_str = if target_module.is_empty() {
        "(root)".to_string()
    } else {
        target_module.join("::")
    };
    println!("Module: {module_str}");
    println!();

    let incoming = collect_incoming(&project, &target_defs);
    print_incoming(&incoming);

    let outgoing = collect_outgoing(&project, &target_defs, &target_file, &def_locations);
    print_outgoing(&outgoing);

    let total_outgoing: usize = outgoing.values().map(std::vec::Vec::len).sum();
    println!("Summary:");
    println!(
        "  {} incoming enrichment point(s) (functions others can call)",
        incoming.len()
    );
    println!("  {total_outgoing} outgoing enrichment point(s) (cross-module calls from here)");

    ExitCode::SUCCESS
}

/// Normalize a path for comparison (canonicalize if possible, otherwise use as-is).
fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_global_refs_finds_names() {
        let term = Term::App(
            Box::new(Term::Global("foo".into())),
            Box::new(Term::Global("bar".into())),
        );
        let refs = collect_global_refs(&term);
        assert!(refs.contains("foo"));
        assert!(refs.contains("bar"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn collect_global_refs_empty_for_non_global() {
        let term = Term::Zero;
        let refs = collect_global_refs(&term);
        assert!(refs.is_empty());
    }

    #[test]
    fn collect_global_refs_nested() {
        let term = Term::Lambda(
            "x".into(),
            tungsten_core::Type::Nat,
            Box::new(Term::App(
                Box::new(Term::Global("helper".into())),
                Box::new(Term::Var("x".into())),
            )),
        );
        let refs = collect_global_refs(&term);
        assert_eq!(refs.len(), 1);
        assert!(refs.contains("helper"));
    }

    #[test]
    fn collect_global_refs_deduplicates() {
        let term = Term::App(
            Box::new(Term::Global("f".into())),
            Box::new(Term::Global("f".into())),
        );
        let refs = collect_global_refs(&term);
        assert_eq!(refs.len(), 1);
    }
}
