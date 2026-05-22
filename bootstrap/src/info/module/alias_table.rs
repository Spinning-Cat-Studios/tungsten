//! `tungsten info module alias-table` — show import alias mappings for a module.
//!
//! For a specific module in the tree, lists each aliased import and shows
//! the local name (alias) → original name + source module mapping.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::ast::{ExpandedUseTree, Item};
use tungsten_bootstrap::driver::{parse_module_tree, ParsedModule};

use super::imports::{find_modules_by_path, resolve_module_target};

/// Entry point for `tungsten info module alias-table <module> <file>`.
pub fn cmd_info_alias_table(module_path: &str, file: &PathBuf, _verbose: bool) -> ExitCode {
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

    // Phase 3: extract alias mappings from use declarations
    println!("Alias table for {module_path} in {}:\n", file.display());

    let aliases = collect_aliases(target);

    if aliases.is_empty() {
        println!("  (no aliased imports in this module)");
        return ExitCode::SUCCESS;
    }

    // Find max widths for alignment
    let max_alias = aliases
        .iter()
        .map(|a| a.alias_name.len())
        .max()
        .unwrap_or(0);
    let max_original = aliases
        .iter()
        .map(|a| a.original_name.len())
        .max()
        .unwrap_or(0);

    println!(
        "  {:<width_a$}  ←  {:<width_o$}  (source)",
        "alias",
        "original",
        width_a = max_alias,
        width_o = max_original
    );
    println!(
        "  {:<width_a$}  ─  {:<width_o$}  ──────",
        "─".repeat(max_alias),
        "─".repeat(max_original),
        width_a = max_alias,
        width_o = max_original
    );

    for entry in &aliases {
        println!(
            "  {:<width_a$}  ←  {:<width_o$}  ({})",
            entry.alias_name,
            entry.original_name,
            entry.source_path,
            width_a = max_alias,
            width_o = max_original
        );
    }

    println!("\n  {} alias(es) found.", aliases.len());
    println!("\n  Note: aliased names suppress the original in this module's scope.");

    ExitCode::SUCCESS
}

/// A single alias entry.
struct AliasEntry {
    alias_name: String,
    original_name: String,
    source_path: String,
}

/// Collect all alias entries from a module's use declarations.
fn collect_aliases(module: &ParsedModule) -> Vec<AliasEntry> {
    let mut aliases = Vec::new();

    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            let expanded = use_decl.tree.expand_all();
            for tree in expanded {
                if let ExpandedUseTree::Alias { path, alias, .. } = tree {
                    let original_name = path
                        .segments
                        .last()
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    let source_path = path
                        .segments
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");
                    aliases.push(AliasEntry {
                        alias_name: alias.name.clone(),
                        original_name,
                        source_path,
                    });
                }
            }
        }
    }

    aliases
}
