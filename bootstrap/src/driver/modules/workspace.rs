//! Workspace-aware module discovery.
//!
//! When checking a single file that's part of a larger project, we need to know
//! about sibling modules for cross-module imports. This module provides functions
//! to discover and parse sibling modules at the workspace root level.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::BuildCache;
use crate::elaborate::ModulePath;

use super::info::{
    build_module_info_recursive, get_module_name, process_pub_use_reexports, ModuleInfo,
};
use super::parse::parse_module_tree;
use super::ParsedModule;

/// Find the workspace root directory for a given file.
///
/// The workspace root is determined by walking up from the file's directory
/// until we find a directory that looks like a project root:
/// 1. Contains `main.tg` (entry point file)
/// 2. OR is the immediate parent of the file being checked
///
/// This allows checking files like `elab/env/mod.tg` to find sibling modules
/// like `lexer` and `parser` at the workspace root level.
pub fn find_workspace_root(file_path: &Path) -> PathBuf {
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    // Walk up from file's directory looking for main.tg
    let mut current = file_dir;
    loop {
        let main_path = current.join("main.tg");
        if main_path.exists() {
            return current.to_path_buf();
        }

        // Move up one directory
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => break, // Reached filesystem root
        }
    }

    // No main.tg found - use the file's immediate parent directory
    file_dir.to_path_buf()
}

/// Discover all module entry points at the workspace root level.
///
/// Returns paths to all `.tg` files and `*/mod.tg` files at the workspace root.
/// These represent top-level modules that can be imported.
pub fn discover_sibling_modules(workspace_root: &Path) -> Vec<PathBuf> {
    let mut modules = Vec::new();

    // Read directory contents
    let entries = match fs::read_dir(workspace_root) {
        Ok(e) => e,
        Err(_) => return modules,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_file() {
            // Include .tg files (but not mod.tg at root level)
            if let Some(ext) = path.extension() {
                if ext == "tg" {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    // Skip mod.tg at root - it's not a standalone module
                    if file_name != "mod" {
                        modules.push(path);
                    }
                }
            }
        } else if path.is_dir() {
            // Check for subdirectory with mod.tg
            let mod_path = path.join("mod.tg");
            if mod_path.exists() {
                modules.push(mod_path);
            }
        }
    }

    modules
}

/// Parse all sibling module trees at the workspace root.
///
/// This is used for workspace-aware resolution: when checking a single file,
/// we also parse its sibling modules so cross-module imports can be resolved.
///
/// Returns a vector of parsed module trees, one for each sibling module.
pub fn parse_workspace_modules(
    workspace_root: &Path,
    cache: Option<&RefCell<BuildCache>>,
) -> Vec<ParsedModule> {
    let module_paths = discover_sibling_modules(workspace_root);
    let mut modules = Vec::new();

    for path in module_paths {
        // Parse each module tree independently
        let mut visited = HashSet::new();
        let mut chain = Vec::new();

        match parse_module_tree(&path, &mut visited, &mut chain, cache) {
            Ok(module) => modules.push(module),
            Err(_) => {
                // Skip modules that fail to parse - they might have syntax errors
                // but we still want to be able to check other modules
            }
        }
    }

    modules
}

/// Build module info from multiple module trees.
///
/// This merges module information from all sibling modules, allowing
/// cross-module imports to be resolved. Each module tree is registered
/// at the root level (e.g., `lexer`, `parser`, `elab` as top-level modules).
pub fn build_workspace_module_info(modules: &[ParsedModule]) -> ModuleInfo {
    let mut info = ModuleInfo::default();

    // First pass: register all modules and items
    for module in modules {
        // Each module becomes a top-level module (e.g., "lexer", "parser")
        let module_name = get_module_name(module);
        let module_path = ModulePath::from_segments(&[module_name]);

        // Use the internal recursive function with the correct path
        build_module_info_recursive(module, &module_path, None, &mut info);
    }

    // Second pass: process pub use re-exports
    for module in modules {
        let module_name = get_module_name(module);
        let module_path = ModulePath::from_segments(&[module_name]);
        process_pub_use_reexports(module, &module_path, &mut info);
    }

    info
}

/// Merge two ModuleInfo structures.
///
/// Used to combine the file-specific module info with workspace-wide module info.
/// The `base` (main file's module info) takes priority for file_to_module mappings,
/// ensuring canonical paths are based on the main module registration.
pub fn merge_module_info(mut base: ModuleInfo, other: ModuleInfo) -> ModuleInfo {
    // Merge file_to_module FIRST (base/main wins - canonical paths)
    // This is critical for canonicalization: main's paths are the canonical ones
    for (file, path) in other.file_to_module {
        base.file_to_module.entry(file).or_insert(path);
    }

    // Merge modules registry
    for (path, contents) in other.modules {
        base.modules.entry(path).or_insert(contents);
    }

    // Merge item_modules (item name → module path)
    // Sort by name for deterministic merge order when same item exists in
    // multiple modules (prevents non-deterministic E0016 diagnostic paths).
    let mut sorted_item_modules: Vec<_> = other.item_modules.into_iter().collect();
    sorted_item_modules.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, path) in sorted_item_modules {
        base.item_modules.entry(name).or_insert(path);
    }

    // Merge module_visibility
    for (path, vis) in other.module_visibility {
        base.module_visibility.entry(path).or_insert(vis);
    }

    // Merge use_statement_modules (keyed by (file_path, span_start))
    for (key, path) in other.use_statement_modules {
        base.use_statement_modules.entry(key).or_insert(path);
    }

    // Merge use_statement_by_span (keyed by full span)
    for (span, path) in other.use_statement_by_span {
        base.use_statement_by_span.entry(span).or_insert(path);
    }

    // Merge module_files
    for (path, file_path) in other.module_files {
        base.module_files.entry(path).or_insert(file_path);
    }

    // Note: item_index_to_file is not merged here - it's set directly from build_combined_ast
    // The index→file mapping is specific to the combined AST being elaborated

    base
}

/// Public wrapper for get_module_name (for verbose logging in driver).
pub fn get_module_name_from_parsed(module: &ParsedModule) -> String {
    get_module_name(module)
}
