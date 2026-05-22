//! Info subcommand implementations for type encoding diagnostics (ADR 20.4.26c).
//!
//! Contains `cmd_info_type_encoding` and `cmd_info_mutual_recursion_groups`.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_type_short;
use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_bootstrap::elaborate::TypeProvenance;

// ═══════════════════════════════════════════════════════════════════════
// tungsten info type-encoding
// ═══════════════════════════════════════════════════════════════════════

/// Display the μ-type encoding of a named type.
///
/// Shows the raw Type tree from Phase 1e encoding cache, the structural
/// display form, and mutual recursion group info if applicable.
pub fn cmd_info_type_encoding(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    show_raw: bool,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    // Check if type exists
    let is_adt = project.adt_types.contains_key(name);
    let is_record = project.record_types.contains_key(name);
    let is_alias = project.type_aliases.contains_key(name);

    if !is_adt && !is_record && !is_alias {
        print_type_not_found(name, &project);
        return ExitCode::FAILURE;
    }

    // Header
    let kind = classify_type_kind(is_adt, is_record);
    println!("Type Encoding: {name} ({kind})");
    println!("{}", "═".repeat(16 + name.len() + kind.len()));
    println!();

    print_cached_encoding(name, &project, show_raw);
    print_display_form(name, &project, show_raw);
    print_mutual_group(name, &project);
    print_provenance(name, &project.type_provenance);

    ExitCode::SUCCESS
}

fn classify_type_kind(is_adt: bool, is_record: bool) -> &'static str {
    if is_adt {
        "ADT"
    } else if is_record {
        "Record"
    } else {
        "Alias"
    }
}

fn print_type_not_found(name: &str, project: &ProjectOutput) {
    eprintln!("Type not found: {name}");
    let mut available: Vec<&str> = project
        .adt_types
        .keys()
        .chain(project.record_types.keys())
        .chain(project.type_aliases.keys())
        .map(std::string::String::as_str)
        .collect();
    available.sort_unstable();
    available.dedup();
    eprintln!("Available types: {}", available.join(", "));
}

fn print_cached_encoding(name: &str, project: &ProjectOutput, show_raw: bool) {
    if let Some(encoded) = project.encoded_types.get(name) {
        println!("Encoding (cached):");
        if show_raw {
            println!("  {}", encoded.display_detailed());
        } else {
            println!("  {encoded}");
        }
        println!();
    } else {
        let is_parameterized = project
            .adt_types
            .get(name)
            .map(|(params, _)| !params.is_empty())
            .or_else(|| {
                project
                    .type_aliases
                    .get(name)
                    .map(|(params, _)| !params.is_empty())
            })
            .unwrap_or(false);

        if is_parameterized {
            println!("Encoding: (parameterized — no cached encoding)");
            if let Some((params, _)) = project.adt_types.get(name) {
                println!("  Type parameters: [{}]", params.join(", "));
            } else if let Some((params, _)) = project.type_aliases.get(name) {
                println!("  Type parameters: [{}]", params.join(", "));
            }
        } else {
            println!("Encoding: (not available)");
        }
        println!();
    }
}

fn print_display_form(name: &str, project: &ProjectOutput, show_raw: bool) {
    if let Some(encoded) = project.encoded_types.get(name) {
        if show_raw {
            println!("Encoding (display):");
            println!("  {encoded}");
            println!();
        }
    }
}

fn print_mutual_group(name: &str, project: &ProjectOutput) {
    if let Some(group) = project.mutual_recursion_groups.get(name) {
        let mut sorted_group = group.clone();
        sorted_group.sort();
        println!("Mutual recursion group: {{{}}}", sorted_group.join(", "));
        println!(
            "Group μ-binder order: [{}]",
            sorted_group
                .iter()
                .map(|n| format!("α_{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!();
    }
}

fn print_provenance(name: &str, provenance: &TypeProvenance) {
    let mu_key = format!("α_{name}");
    if let Some(origin) = provenance.mu_origins.get(&mu_key) {
        println!("Provenance:");
        println!("  ADT name:     {}", origin.adt_name);
        if !origin.type_args.is_empty() {
            let args: Vec<String> = origin.type_args.iter().map(format_type_short).collect();
            println!("  Type args:    [{}]", args.join(", "));
        }
        println!("  Constructors: [{}]", origin.constructors.join(", "));
        println!();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// tungsten info mutual-recursion-groups
// ═══════════════════════════════════════════════════════════════════════

/// Display the strongly connected components (SCCs) of the type dependency graph.
///
/// Reuses the `TypeGraph` + `tarjan_scc` infrastructure from `doctor/audit_mutual_types`
/// but focuses on the encoding-relevant SCC groups and μ-binder ordering.
pub fn cmd_info_mutual_recursion_groups(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    use tungsten_bootstrap::doctor::audit_mutual_types::{scc, type_graph::TypeGraph};

    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    // Build type dependency graph and compute SCCs
    let graph = TypeGraph::build(&project.adt_types);

    if verbose {
        eprintln!(
            "Type graph: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    }

    let sccs = scc::tarjan_scc(&graph);

    // Classify SCCs
    let mut mutual_groups: Vec<Vec<String>> = Vec::new();
    let mut self_recursive: Vec<String> = Vec::new();

    for component in &sccs {
        if component.len() == 1 {
            let name = &component[0];
            if graph.has_edge(name, name) {
                self_recursive.push(name.clone());
            }
        } else {
            let mut sorted = component.clone();
            sorted.sort();
            mutual_groups.push(sorted);
        }
    }

    // Sort groups by size (largest first), then alphabetically
    mutual_groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    self_recursive.sort();

    // Output
    println!("Mutual Recursion Groups");
    println!("═══════════════════════");
    println!();

    if mutual_groups.is_empty() {
        println!("No mutually recursive type groups found.");
    } else {
        for (i, group) in mutual_groups.iter().enumerate() {
            println!("SCC Group {} ({} types):", i + 1, group.len());
            println!("  {{{}}}", group.join(", "));
            println!(
                "  μ-binder order: [{}]",
                group
                    .iter()
                    .map(|n| format!("α_{n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            // Show dependency edges within the group
            println!("  Dependency edges:");
            for from in group {
                let mut targets: Vec<&str> = group
                    .iter()
                    .filter(|to| *to != from && graph.has_edge(from, to))
                    .map(std::string::String::as_str)
                    .collect();
                targets.sort_unstable();
                if !targets.is_empty() {
                    println!("    {from} → {}", targets.join(", "));
                }
            }
            println!();
        }
    }

    if !self_recursive.is_empty() {
        println!(
            "Self-recursive types (not in mutual group): {}",
            self_recursive.len()
        );
        for name in &self_recursive {
            println!("  {name}");
        }
        println!();
    }

    ExitCode::SUCCESS
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
    fn test_type_encoding_simple_adt() {
        let (_dir, path) =
            make_temp_file("type Color = Red | Green | Blue\nfn main() -> Nat { 0 }");
        let result = cmd_info_type_encoding("Color", &path, false, 20, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_type_encoding_recursive_adt() {
        let (_dir, path) =
            make_temp_file("type List<T> = Nil | Cons(T, List<T>)\nfn main() -> Nat { 0 }");
        // Parameterized type — no cached encoding
        let result = cmd_info_type_encoding("List", &path, false, 20, false);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_type_encoding_show_raw() {
        let (_dir, path) =
            make_temp_file("type Color = Red | Green | Blue\nfn main() -> Nat { 0 }");
        let result = cmd_info_type_encoding("Color", &path, false, 20, true);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_type_encoding_not_found() {
        let (_dir, path) = make_temp_file("fn main() -> Nat { 0 }");
        let result = cmd_info_type_encoding("NonExistent", &path, false, 20, false);
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[test]
    fn test_mutual_recursion_groups_no_mutual() {
        let (_dir, path) =
            make_temp_file("type Color = Red | Green | Blue\nfn main() -> Nat { 0 }");
        let result = cmd_info_mutual_recursion_groups(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_mutual_recursion_groups_self_recursive() {
        let (_dir, path) =
            make_temp_file("type List<T> = Nil | Cons(T, List<T>)\nfn main() -> Nat { 0 }");
        let result = cmd_info_mutual_recursion_groups(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_mutual_recursion_groups_mutual() {
        let (_dir, path) = make_temp_file(
            r#"
type Expr = Lit(Nat) | Typed(TypeExpr)
type TypeExpr = TyNat | TyOf(Expr)
fn main() -> Nat { 0 }
"#,
        );
        let result = cmd_info_mutual_recursion_groups(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }
}
