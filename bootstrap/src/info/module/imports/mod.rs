//! `tungsten info imports` — show import resolution status for a module.
//!
//! For a specific module in the tree, lists each `use` declaration and reports
//! whether each imported name resolved to a full definition or a stub.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::ast::Item;
use tungsten_bootstrap::driver::{
    self, build_module_info, get_module_name_from_parsed, parse_module_tree, ParsedModule,
};

/// Entry point for `tungsten info imports <module> <file>`.
pub fn cmd_info_imports(
    module_path: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    // Phase 1: parse module tree
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Phase 2: find the target module by qualified path
    let candidates = find_modules_by_path(&module_tree, module_path, "");
    let target = match resolve_module_target(&candidates, module_path) {
        Some(t) => t,
        None => return ExitCode::FAILURE,
    };

    // Phase 3: elaborate to get resolved definitions
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Phase 4: extract use declarations and report resolution status
    let module_info = build_module_info(&module_tree);
    let all_constructors: BTreeMap<String, String> = module_info
        .modules
        .values()
        .flat_map(|c| {
            c.constructor_details
                .iter()
                .map(|(name, detail)| (name.clone(), detail.type_name.clone()))
        })
        .collect();

    println!("Imports for {module_path} in {}:\n", file.display());

    let stub_count = render_import_status(target, &project, &all_constructors);

    if stub_count > 0 {
        println!(
            "⚠ {stub_count} import(s) resolved to a stub — \
             this module may see incomplete type information."
        );
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Render each use declaration and its resolution status. Returns the stub count.
fn render_import_status(
    target: &ParsedModule,
    project: &driver::ProjectOutput,
    all_constructors: &BTreeMap<String, String>,
) -> usize {
    let mut stub_count = 0;
    let mut found_uses = false;

    for item in &target.source_file.items {
        if let Item::Use(use_decl) = item {
            found_uses = true;
            let paths = use_decl.tree.expand();
            let imported = match &paths {
                tungsten_bootstrap::ast::ExpandedUseTree::Paths(paths) => paths
                    .iter()
                    .map(|p| {
                        p.segments
                            .last()
                            .map(|s| s.name.clone())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>(),
                tungsten_bootstrap::ast::ExpandedUseTree::Glob { .. } => {
                    println!("  use ... (glob import — cannot enumerate)");
                    continue;
                }
                tungsten_bootstrap::ast::ExpandedUseTree::Alias { path, alias, .. } => {
                    vec![format!(
                        "{} (as {})",
                        path.segments.last().map_or("", |s| s.name.as_str()),
                        alias.name
                    )]
                }
            };

            let source_path = format_use_path(use_decl);
            println!("  use {source_path}");

            for name in &imported {
                let status = classify_import(name, project, all_constructors);
                let marker = if status.is_stub { " ⚠" } else { "" };
                println!("    {name:<20} → {}{marker}", status.description);
                if status.is_stub {
                    stub_count += 1;
                }
            }
            println!();
        }
    }

    if !found_uses {
        println!("  (no use declarations in this module)");
    }

    stub_count
}

/// Result of classifying an imported name.
struct ImportStatus {
    description: String,
    is_stub: bool,
}

/// Classify an imported name against the elaboration output.
fn classify_import(
    name: &str,
    project: &driver::ProjectOutput,
    constructors: &BTreeMap<String, String>,
) -> ImportStatus {
    if let Some((params, ctors)) = project.adt_types.get(name) {
        return ImportStatus {
            description: format!(
                "full def (ADT, {} variant(s), {} param(s))",
                ctors.len(),
                params.len()
            ),
            is_stub: false,
        };
    }
    if let Some(fields) = project.record_types.get(name) {
        return ImportStatus {
            description: format!("full def (record, {} field(s))", fields.len()),
            is_stub: false,
        };
    }
    if let Some((params, _ty)) = project.type_aliases.get(name) {
        return ImportStatus {
            description: format!("full def (alias, {} param(s))", params.len()),
            is_stub: false,
        };
    }
    if let Some(parent) = constructors.get(name) {
        return ImportStatus {
            description: format!("constructor (parent: {parent})"),
            is_stub: false,
        };
    }
    // Check if it's a value definition
    if project.defs.iter().any(|d| d.name == name) {
        return ImportStatus {
            description: "full def (value)".to_string(),
            is_stub: false,
        };
    }
    ImportStatus {
        description: "not resolved (stub or missing)".to_string(),
        is_stub: true,
    }
}

/// Format a use declaration's path for display.
fn format_use_path(use_decl: &tungsten_bootstrap::ast::UseDecl) -> String {
    let paths = use_decl.tree.expand();
    match &paths {
        tungsten_bootstrap::ast::ExpandedUseTree::Paths(paths) => {
            if paths.len() == 1 {
                paths[0]
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::")
            } else {
                // Find common prefix
                let first = &paths[0].segments;
                let prefix_len = (0..first.len())
                    .take_while(|&i| {
                        paths
                            .iter()
                            .all(|p| p.segments.get(i).map(|s| &s.name) == Some(&first[i].name))
                    })
                    .count();
                let prefix: Vec<_> = first[..prefix_len]
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect();
                let suffixes: Vec<_> = paths
                    .iter()
                    .map(|p| {
                        p.segments[prefix_len..]
                            .iter()
                            .map(|s| s.name.as_str())
                            .collect::<Vec<_>>()
                            .join("::")
                    })
                    .collect();
                format!("{}::{{{}}}", prefix.join("::"), suffixes.join(", "))
            }
        }
        tungsten_bootstrap::ast::ExpandedUseTree::Glob { prefix, .. } => {
            let p: Vec<_> = prefix.segments.iter().map(|s| s.name.as_str()).collect();
            format!("{}::*", p.join("::"))
        }
        tungsten_bootstrap::ast::ExpandedUseTree::Alias { path, alias, .. } => {
            let p: Vec<_> = path.segments.iter().map(|s| s.name.as_str()).collect();
            format!("{} as {}", p.join("::"), alias.name)
        }
    }
}

/// Find modules matching a qualified path (or suffix).
pub fn find_modules_by_path<'a>(
    module: &'a ParsedModule,
    target: &str,
    parent_path: &str,
) -> Vec<(String, &'a ParsedModule)> {
    let name = get_module_name_from_parsed(module);
    let my_path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}::{name}")
    };

    let mut results = Vec::new();

    // Check if this module matches (exact match or suffix match)
    if my_path == target || my_path.ends_with(&format!("::{target}")) || name == target {
        results.push((my_path.clone(), module));
    }

    for child in &module.submodules {
        results.extend(find_modules_by_path(child, target, &my_path));
    }
    results
}

/// Resolve the target module, handling ambiguity.
pub fn resolve_module_target<'a>(
    candidates: &[(String, &'a ParsedModule)],
    target: &str,
) -> Option<&'a ParsedModule> {
    match candidates.len() {
        0 => {
            eprintln!("error: module '{target}' not found in module tree");
            None
        }
        1 => Some(candidates[0].1),
        _ => {
            // Check for exact match first
            if let Some((_, m)) = candidates.iter().find(|(path, _)| path == target) {
                return Some(m);
            }
            eprintln!("error: '{target}' is ambiguous. Candidates:");
            for (path, _) in candidates {
                eprintln!("  {path}");
            }
            eprintln!("\nUse the fully qualified path to disambiguate.");
            None
        }
    }
}

#[cfg(test)]
mod tests;
