//! `tungsten info module tree` — visualize module hierarchy and elaboration order.
//!
//! Parses the module tree, computes the dependency-sorted elaboration sequence,
//! and renders: (1) indented tree view, (2) flat elaboration order,
//! (3) cross-branch dependencies. Cost ≤ 2 (parse only, no elaboration).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::ast::Item;
use tungsten_bootstrap::driver::{
    get_module_name_from_parsed, parse_module_tree, sort_submodules_by_deps, use_first_segments,
    ParsedModule,
};

/// Entry point for `tungsten info module tree <file>`.
pub fn cmd_info_module_tree(file: &PathBuf, _verbose: bool) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("Module tree for {}:\n", file.display());

    let root_name = get_module_name_from_parsed(&module_tree);
    println!("  {root_name} (root)");
    render_children(&module_tree, "    ");

    let mut order = Vec::new();
    compute_elaboration_order(&module_tree, "", &mut order);
    println!("\nElaboration order:");
    for (i, path) in order.iter().enumerate() {
        println!("  {}. {path}", i + 1);
    }

    let all_names = collect_all_module_names(&module_tree, "");
    let cross_deps = find_cross_branch_deps(&module_tree, "", &all_names);
    if !cross_deps.is_empty() {
        println!("\nCross-branch dependencies:");
        for (from, to) in &cross_deps {
            println!("  {from} → {to}  (cross-branch)");
        }
    }

    ExitCode::SUCCESS
}

/// Render children of a module as an indented tree with box-drawing characters.
fn render_children(module: &ParsedModule, indent: &str) {
    let count = module.submodules.len();
    for (i, child) in module.submodules.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let name = get_module_name_from_parsed(child);
        println!("{indent}{connector}{name}");
        let child_indent = if is_last {
            format!("{indent}    ")
        } else {
            format!("{indent}│   ")
        };
        render_children(child, &child_indent);
    }
}

/// Compute the post-order elaboration sequence with dependency-sorted siblings.
fn compute_elaboration_order(module: &ParsedModule, parent_path: &str, order: &mut Vec<String>) {
    let name = get_module_name_from_parsed(module);
    let my_path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}::{name}")
    };

    let sorted_indices = sort_submodules_by_deps(&module.submodules);
    for &idx in &sorted_indices {
        compute_elaboration_order(&module.submodules[idx], &my_path, order);
    }

    order.push(my_path);
}

/// Collect all fully-qualified module names in the tree.
fn collect_all_module_names(module: &ParsedModule, parent_path: &str) -> HashSet<String> {
    let name = get_module_name_from_parsed(module);
    let my_path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}::{name}")
    };
    let mut names = HashSet::new();
    names.insert(my_path.clone());
    for child in &module.submodules {
        names.extend(collect_all_module_names(child, &my_path));
    }
    names
}

/// Find cross-branch dependencies: use paths that reference sibling branches.
fn find_cross_branch_deps(
    module: &ParsedModule,
    parent_path: &str,
    all_names: &HashSet<String>,
) -> Vec<(String, String)> {
    let name = get_module_name_from_parsed(module);
    let my_path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}::{name}")
    };

    let mut deps = Vec::new();

    // Build a map of direct children names for this module
    let child_names: HashMap<String, String> = module
        .submodules
        .iter()
        .map(|c| {
            let cname = get_module_name_from_parsed(c);
            let cpath = format!("{my_path}::{cname}");
            (cname, cpath)
        })
        .collect();

    // Check each child's use declarations for cross-branch refs
    for child in &module.submodules {
        let child_name = get_module_name_from_parsed(child);
        let child_path = format!("{my_path}::{child_name}");

        for item in &child.source_file.items {
            if let Item::Use(use_decl) = item {
                for seg in use_first_segments(use_decl) {
                    // If the first segment names a sibling, it's a cross-branch dep
                    if let Some(sibling_path) = child_names.get(&seg) {
                        if sibling_path != &child_path {
                            let entry = (child_path.clone(), sibling_path.clone());
                            if !deps.contains(&entry) {
                                deps.push(entry);
                            }
                        }
                    }
                }
            }
        }

        // Recurse into children
        deps.extend(find_cross_branch_deps(child, &my_path, all_names));
    }

    deps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tungsten_bootstrap::ast::SourceFile;
    use tungsten_bootstrap::driver::ParsedModule;
    use tungsten_bootstrap::span::Span;

    fn leaf_module(name: &str) -> ParsedModule {
        ParsedModule {
            path: PathBuf::from(format!("{name}.tg")),
            visibility: tungsten_bootstrap::ast::Visibility::Public,
            source_file: SourceFile {
                items: vec![],
                span: Span { start: 0, end: 0 },
            },
            submodules: vec![],
        }
    }

    #[test]
    fn elaboration_order_single_module() {
        let m = leaf_module("main");
        let mut order = Vec::new();
        compute_elaboration_order(&m, "", &mut order);
        assert_eq!(order, vec!["main"]);
    }

    #[test]
    fn elaboration_order_children_before_parent() {
        let m = ParsedModule {
            path: PathBuf::from("main.tg"),
            visibility: tungsten_bootstrap::ast::Visibility::Public,
            source_file: SourceFile {
                items: vec![],
                span: Span { start: 0, end: 0 },
            },
            submodules: vec![leaf_module("lexer"), leaf_module("parser")],
        };
        let mut order = Vec::new();
        compute_elaboration_order(&m, "", &mut order);
        // Children elaborated before parent (post-order)
        assert_eq!(order.last().unwrap(), "main");
        assert!(order.contains(&"main::lexer".to_string()));
        assert!(order.contains(&"main::parser".to_string()));
    }

    #[test]
    fn collect_names_nested() {
        let m = ParsedModule {
            path: PathBuf::from("main.tg"),
            visibility: tungsten_bootstrap::ast::Visibility::Public,
            source_file: SourceFile {
                items: vec![],
                span: Span { start: 0, end: 0 },
            },
            submodules: vec![ParsedModule {
                path: PathBuf::from("elab/mod.tg"),
                visibility: tungsten_bootstrap::ast::Visibility::Public,
                source_file: SourceFile {
                    items: vec![],
                    span: Span { start: 0, end: 0 },
                },
                submodules: vec![leaf_module("env")],
            }],
        };
        let names = collect_all_module_names(&m, "");
        assert!(names.contains("main"));
        assert!(names.contains("main::elab"));
        assert!(names.contains("main::elab::env"));
    }

    #[test]
    fn find_cross_branch_deps_detects_sibling_use() {
        use tungsten_bootstrap::ast::{Ident, Item, Path as AstPath, UseDecl, UseTree, Visibility};

        let s = Span { start: 0, end: 0 };

        // Build a child "driver" that has `use parser::something;`
        let use_decl = UseDecl {
            visibility: Visibility::Private,
            tree: UseTree::Path(AstPath {
                segments: vec![Ident::new("parser", s), Ident::new("Expr", s)],
                span: s,
            }),
            span: s,
        };

        let driver_mod = ParsedModule {
            path: PathBuf::from("driver.tg"),
            visibility: Visibility::Public,
            source_file: SourceFile {
                items: vec![Item::Use(use_decl)],
                span: s,
            },
            submodules: vec![],
        };

        let root = ParsedModule {
            path: PathBuf::from("main.tg"),
            visibility: Visibility::Public,
            source_file: SourceFile {
                items: vec![],
                span: s,
            },
            submodules: vec![leaf_module("parser"), driver_mod],
        };

        let all_names = collect_all_module_names(&root, "");
        let deps = find_cross_branch_deps(&root, "", &all_names);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].0, "main::driver");
        assert_eq!(deps[0].1, "main::parser");
    }
}
