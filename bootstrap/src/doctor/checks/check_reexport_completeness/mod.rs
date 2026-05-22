//! `tungsten doctor check reexport-completeness` — detect broken pub use re-exports.
//!
//! Walks the module tree after re-export processing and checks that every
//! `pub use` declaration actually copied items. Reports declarations that
//! resolved to zero items or had missing named imports.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::ast::{ExpandedUseTree, Item, Visibility};
use crate::driver::modules::{
    build_module_info, get_module_name_from_parsed, parse_module_tree, resolve_pub_use_module,
    ModuleInfo,
};
use crate::driver::ParsedModule;
use crate::elaborate::{ModuleContents, ModulePath};

/// A single re-export issue found during the check.
#[derive(Debug)]
pub(crate) enum ReexportIssue {
    /// Source module path did not resolve.
    UnresolvedModule {
        module_path: ModulePath,
        source_segments: Vec<String>,
        declaration: String,
    },
    /// Glob re-export copied zero items but source has items.
    EmptyGlob {
        module_path: ModulePath,
        source_path: ModulePath,
        source_item_count: usize,
    },
    /// Named item was missing from the source module.
    MissingNamedItem {
        module_path: ModulePath,
        source_path: ModulePath,
        item_name: String,
    },
    /// Named item existed in source but was not copied into target.
    NotCopied {
        module_path: ModulePath,
        source_path: ModulePath,
        item_name: String,
    },
}

/// Entry point for `tungsten doctor check reexport-completeness <file>`.
pub fn cmd_check_reexport_completeness(file: &PathBuf, verbose: bool) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let info = build_module_info(&module_tree);
    let issues = check_reexports(&module_tree, &ModulePath::root(), &info);

    if issues.is_empty() {
        let total = count_pub_use_decls(&module_tree);
        println!("✓ All {total} pub use re-export(s) are complete.");
        return ExitCode::SUCCESS;
    }

    println!("Checking re-export completeness...\n");
    for issue in &issues {
        print_issue(issue, verbose);
    }

    let total = count_pub_use_decls(&module_tree);
    let ok = total.saturating_sub(issues.len());
    println!(
        "Summary: {} incomplete re-export(s), {} OK",
        issues.len(),
        ok
    );
    ExitCode::FAILURE
}

/// Walk the module tree and check each pub use declaration for completeness.
pub(crate) fn check_reexports(
    module: &ParsedModule,
    current_path: &ModulePath,
    info: &ModuleInfo,
) -> Vec<ReexportIssue> {
    let mut issues = Vec::new();

    for submodule in &module.submodules {
        let name = get_module_name_from_parsed(submodule);
        let child_path = current_path.child(name);
        issues.extend(check_reexports(submodule, &child_path, info));
    }

    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            if !matches!(use_decl.visibility, Visibility::Public | Visibility::Crate) {
                continue;
            }
            match use_decl.tree.expand() {
                ExpandedUseTree::Paths(paths) => {
                    check_named_reexports(&paths, current_path, info, &mut issues);
                }
                ExpandedUseTree::Glob { prefix, .. } => {
                    check_glob_reexport(&prefix, current_path, info, &mut issues);
                }
                ExpandedUseTree::Alias { .. } => {
                    // Aliased re-exports: treated as named (no completeness check needed)
                }
            }
        }
    }

    issues
}

/// Check named re-exports (`pub use foo::{bar, baz}`).
fn check_named_reexports(
    paths: &[crate::ast::Path],
    current_path: &ModulePath,
    info: &ModuleInfo,
    issues: &mut Vec<ReexportIssue>,
) {
    for path in paths {
        if path.segments.len() < 2 {
            continue;
        }
        let item_name = path.segments.last().unwrap().name.clone();
        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
            .iter()
            .map(|s| s.name.clone())
            .collect();

        let source = resolve_pub_use_module(&module_segments, current_path, info);
        match source {
            None => {
                let decl_path = path.segments[..path.segments.len() - 1]
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                issues.push(ReexportIssue::UnresolvedModule {
                    module_path: current_path.clone(),
                    source_segments: module_segments,
                    declaration: format!("pub use {decl_path}::{{{item_name}}}"),
                });
            }
            Some(src_path) => {
                let src_contents = info.modules.get(&src_path);
                let item_in_source = src_contents.map_or(false, |c| contains_item(c, &item_name));
                if !item_in_source {
                    issues.push(ReexportIssue::MissingNamedItem {
                        module_path: current_path.clone(),
                        source_path: src_path,
                        item_name,
                    });
                } else {
                    let tgt_contents = info.modules.get(current_path);
                    let item_in_target =
                        tgt_contents.map_or(false, |c| contains_item(c, &item_name));
                    if !item_in_target {
                        issues.push(ReexportIssue::NotCopied {
                            module_path: current_path.clone(),
                            source_path: src_path,
                            item_name,
                        });
                    }
                }
            }
        }
    }
}

/// Check a glob re-export (`pub use foo::*`).
fn check_glob_reexport(
    prefix: &crate::ast::Path,
    current_path: &ModulePath,
    info: &ModuleInfo,
    issues: &mut Vec<ReexportIssue>,
) {
    let module_segments: Vec<String> = prefix.segments.iter().map(|s| s.name.clone()).collect();

    let source = resolve_pub_use_module(&module_segments, current_path, info);
    match source {
        None => {
            let decl_path = prefix
                .segments
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join("::");
            issues.push(ReexportIssue::UnresolvedModule {
                module_path: current_path.clone(),
                source_segments: module_segments,
                declaration: format!("pub use {decl_path}::*"),
            });
        }
        Some(src_path) => {
            let src_count = info.modules.get(&src_path).map_or(0, |c| item_count(c));
            let tgt_contents = info.modules.get(current_path);
            let copied = info.modules.get(&src_path).map_or(0, |src| {
                tgt_contents.map_or(0, |tgt| count_copied(src, tgt))
            });
            if src_count > 0 && copied == 0 {
                issues.push(ReexportIssue::EmptyGlob {
                    module_path: current_path.clone(),
                    source_path: src_path,
                    source_item_count: src_count,
                });
            }
        }
    }
}

fn contains_item(contents: &ModuleContents, name: &str) -> bool {
    contents.types.iter().any(|n| n == name)
        || contents.values.iter().any(|n| n == name)
        || contents.constructors.iter().any(|n| n == name)
}

fn item_count(contents: &ModuleContents) -> usize {
    contents.types.len() + contents.values.len() + contents.constructors.len()
}

fn count_copied(src: &ModuleContents, tgt: &ModuleContents) -> usize {
    let mut count = 0;
    for name in &src.types {
        if tgt.types.iter().any(|n| n == name) {
            count += 1;
        }
    }
    for name in &src.values {
        if tgt.values.iter().any(|n| n == name) {
            count += 1;
        }
    }
    for name in &src.constructors {
        if tgt.constructors.iter().any(|n| n == name) {
            count += 1;
        }
    }
    count
}

fn count_pub_use_decls(module: &ParsedModule) -> usize {
    let mut count = 0;
    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            if matches!(use_decl.visibility, Visibility::Public | Visibility::Crate) {
                count += 1;
            }
        }
    }
    for sub in &module.submodules {
        count += count_pub_use_decls(sub);
    }
    count
}

fn print_issue(issue: &ReexportIssue, _verbose: bool) {
    match issue {
        ReexportIssue::UnresolvedModule {
            module_path,
            declaration,
            ..
        } => {
            println!("⚠ {module_path}");
            println!("  {declaration} → source module not found\n");
        }
        ReexportIssue::EmptyGlob {
            module_path,
            source_path,
            source_item_count,
        } => {
            println!("⚠ {module_path}");
            println!("  pub use {source_path}::* → copied 0 items (expected ≥ 1)");
            println!("  Source module {source_path} has {source_item_count} items\n");
        }
        ReexportIssue::MissingNamedItem {
            module_path,
            source_path,
            item_name,
        } => {
            println!("⚠ {module_path}");
            println!("  pub use {source_path}::{{{item_name}}} → item not found in source\n");
        }
        ReexportIssue::NotCopied {
            module_path,
            source_path,
            item_name,
        } => {
            println!("⚠ {module_path}");
            println!("  pub use {source_path}::{{{item_name}}} → item exists in source but was not copied\n");
        }
    }
}

#[cfg(test)]
mod tests;
