//! `tungsten info reexport-chain` — trace re-export paths for a module's items.
//!
//! Given a module path, shows how its items propagate through `pub use`
//! declarations in ancestor modules.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::ast::{ExpandedUseTree, Item, Visibility};
use tungsten_bootstrap::driver::{
    build_module_info, get_module_name_from_parsed, parse_module_tree, resolve_pub_use_module,
    ModuleInfo, ParsedModule,
};
use tungsten_bootstrap::elaborate::ModulePath;

/// A single re-export hop: module M re-exports items from source via pub use.
struct ReexportHop {
    /// Module that contains the pub use declaration.
    target: ModulePath,
    /// Items that were re-exported in this hop.
    items: Vec<String>,
    /// Whether this was a glob (`*`) or named re-export.
    kind: ReexportKind,
}

enum ReexportKind {
    Glob,
    Named,
}

/// Entry point for `tungsten info reexport-chain <module> <file>`.
pub fn cmd_info_reexport_chain(module: &str, file: &PathBuf, verbose: bool) -> ExitCode {
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

    let target_path =
        ModulePath::from_segments(&module.split("::").map(String::from).collect::<Vec<_>>());

    // Check the module exists
    let source_contents = if let Some(c) = info.modules.get(&target_path) {
        c
    } else {
        eprintln!("error: module '{module}' not found in module tree");
        return ExitCode::FAILURE;
    };

    let source_items: Vec<String> = source_contents
        .types
        .iter()
        .chain(source_contents.values.iter())
        .chain(source_contents.constructors.iter())
        .cloned()
        .collect();

    if source_items.is_empty() {
        println!("Module '{module}' has no items to trace.");
        return ExitCode::SUCCESS;
    }

    println!("Re-export chain for module '{module}':");
    println!("  Source items: {}", source_items.join(", "));
    println!();

    let hops = find_reexport_hops(&module_tree, &ModulePath::root(), &target_path, &info);

    if hops.is_empty() {
        println!("  No re-exports found for this module.");
    } else {
        for hop in &hops {
            let kind = match hop.kind {
                ReexportKind::Glob => "glob (*)",
                ReexportKind::Named => "named",
            };
            println!("  → {} ({kind})", hop.target);
            for item in &hop.items {
                println!("      {item}");
            }
        }
    }

    if verbose {
        println!("\nTotal: {} hop(s)", hops.len());
    }

    ExitCode::SUCCESS
}

/// Walk the module tree and find modules that re-export from the target path.
fn find_reexport_hops(
    module: &ParsedModule,
    current_path: &ModulePath,
    target_path: &ModulePath,
    info: &ModuleInfo,
) -> Vec<ReexportHop> {
    let mut hops = Vec::new();

    // Check submodules
    for submodule in &module.submodules {
        let name = get_module_name_from_parsed(submodule);
        let child_path = current_path.child(name);
        hops.extend(find_reexport_hops(
            submodule,
            &child_path,
            target_path,
            info,
        ));
    }

    // Check pub use declarations in this module
    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            if !matches!(use_decl.visibility, Visibility::Public | Visibility::Crate) {
                continue;
            }

            match use_decl.tree.expand() {
                ExpandedUseTree::Glob { prefix, .. } => {
                    let segments: Vec<String> =
                        prefix.segments.iter().map(|s| s.name.clone()).collect();
                    let src = resolve_pub_use_module(&segments, current_path, info);
                    if src.as_ref() == Some(target_path) {
                        // This module glob-re-exports from target
                        let copied = find_copied_items(target_path, current_path, info);
                        if !copied.is_empty() {
                            hops.push(ReexportHop {
                                target: current_path.clone(),
                                items: copied,
                                kind: ReexportKind::Glob,
                            });
                        }
                    }
                }
                ExpandedUseTree::Paths(paths) => {
                    for path in paths {
                        if path.segments.len() < 2 {
                            continue;
                        }
                        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
                            .iter()
                            .map(|s| s.name.clone())
                            .collect();
                        let src = resolve_pub_use_module(&module_segments, current_path, info);
                        if src.as_ref() == Some(target_path) {
                            let item_name = path.segments.last().unwrap().name.clone();
                            hops.push(ReexportHop {
                                target: current_path.clone(),
                                items: vec![item_name],
                                kind: ReexportKind::Named,
                            });
                        }
                    }
                }
                ExpandedUseTree::Alias { path, alias, .. } => {
                    if path.segments.len() >= 2 {
                        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
                            .iter()
                            .map(|s| s.name.clone())
                            .collect();
                        let src = resolve_pub_use_module(&module_segments, current_path, info);
                        if src.as_ref() == Some(target_path) {
                            hops.push(ReexportHop {
                                target: current_path.clone(),
                                items: vec![alias.name.clone()],
                                kind: ReexportKind::Named,
                            });
                        }
                    }
                }
            }
        }
    }

    hops
}

/// Find items from source that appear in target after re-export processing.
fn find_copied_items(
    source_path: &ModulePath,
    target_path: &ModulePath,
    info: &ModuleInfo,
) -> Vec<String> {
    let src = match info.modules.get(source_path) {
        Some(c) => c,
        None => return Vec::new(),
    };
    let tgt = match info.modules.get(target_path) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut copied = Vec::new();
    for name in &src.types {
        if tgt.types.iter().any(|n| n == name) {
            copied.push(name.clone());
        }
    }
    for name in &src.values {
        if tgt.values.iter().any(|n| n == name) {
            copied.push(name.clone());
        }
    }
    for name in &src.constructors {
        if tgt.constructors.iter().any(|n| n == name) {
            copied.push(name.clone());
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Two-hop chain: leaf → mid → top.
    #[test]
    fn test_two_hop_reexport_chain() {
        let dir = TempDir::new().unwrap();

        std::fs::write(dir.path().join("main.tg"), "mod top;\n").unwrap();

        let top_dir = dir.path().join("top");
        std::fs::create_dir(&top_dir).unwrap();
        std::fs::write(top_dir.join("mod.tg"), "mod mid;\npub use top::mid::*;\n").unwrap();

        let mid_dir = top_dir.join("mid");
        std::fs::create_dir(&mid_dir).unwrap();
        std::fs::write(
            mid_dir.join("mod.tg"),
            "mod leaf;\npub use top::mid::leaf::*;\n",
        )
        .unwrap();

        std::fs::write(mid_dir.join("leaf.tg"), "pub type Quux = { val: Nat }\n").unwrap();

        let main_path = dir.path().join("main.tg");
        let mut visited = HashSet::new();
        let mut chain = Vec::new();
        let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
        let info = build_module_info(&tree);

        let leaf_path = ModulePath::root()
            .child("top".to_string())
            .child("mid".to_string())
            .child("leaf".to_string());

        let hops = find_reexport_hops(&tree, &ModulePath::root(), &leaf_path, &info);

        // mid re-exports from leaf, top re-exports from mid
        // But we only trace direct re-exports FROM leaf, so just mid
        assert!(
            hops.iter().any(|h| h.target
                == ModulePath::root()
                    .child("top".to_string())
                    .child("mid".to_string())),
            "mid should re-export from leaf"
        );
        assert!(!hops.is_empty(), "should find at least one re-export hop");
    }

    /// Module exists but nobody re-exports from it → empty result.
    #[test]
    fn test_no_reexports_found() {
        let dir = TempDir::new().unwrap();

        std::fs::write(dir.path().join("main.tg"), "mod child;\n").unwrap();

        let child_dir = dir.path().join("child");
        std::fs::create_dir(&child_dir).unwrap();
        std::fs::write(child_dir.join("mod.tg"), "pub type Foo = { x: Nat }\n").unwrap();

        let main_path = dir.path().join("main.tg");
        let mut visited = HashSet::new();
        let mut chain = Vec::new();
        let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
        let info = build_module_info(&tree);

        let child_path = ModulePath::root().child("child".to_string());
        let hops = find_reexport_hops(&tree, &ModulePath::root(), &child_path, &info);

        assert!(
            hops.is_empty(),
            "module with no re-exporters should produce 0 hops, got: {}",
            hops.len()
        );
    }
}
