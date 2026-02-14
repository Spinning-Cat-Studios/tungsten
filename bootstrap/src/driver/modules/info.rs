//! Module info extraction for the elaborator.
//!
//! This module builds the `ModuleInfo` structure that the elaborator uses
//! to resolve items across modules.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::{ExpandedUseTree, Item, Visibility};
use crate::elaborate::{ModuleContents, ModulePath};

use super::ParsedModule;

/// Information about which module each item belongs to.
#[derive(Debug, Clone, Default)]
pub struct ModuleInfo {
    /// Module registry: maps module paths to their contents
    pub modules: HashMap<ModulePath, ModuleContents>,
    /// Item to module mapping: item name → module path
    pub item_modules: HashMap<String, ModulePath>,
    /// Module visibility: maps module paths to (visibility, parent_module)
    pub module_visibility: HashMap<ModulePath, (Visibility, Option<ModulePath>)>,
    /// Use statement locations: (file_path, span_start) → module path
    /// Used to determine which module a use statement belongs to.
    /// The file_path is needed because span offsets can overlap across files.
    pub use_statement_modules: HashMap<(PathBuf, u32), ModulePath>,
    /// Alternative: full span → module path mapping
    /// This uses the full (start, end) span which is more likely to be unique
    pub use_statement_by_span: HashMap<(u32, u32), ModulePath>,
    /// Module to file path mapping: module path → source file path
    /// Used for multi-file error reporting
    pub module_files: HashMap<ModulePath, PathBuf>,
    /// File to canonical module path mapping: source file → canonical module path
    /// Used for path canonicalization to resolve path prefix mismatches.
    /// When the same logical module is registered under different paths
    /// (e.g., `main::parser` vs `parser`), this provides the canonical path.
    pub file_to_module: HashMap<PathBuf, ModulePath>,
    /// Item index to file path mapping: index in combined AST → file path
    /// Used to determine which file an item came from in the combined AST.
    /// Unlike span-based mapping, indices are always unique.
    pub item_index_to_file: Vec<PathBuf>,
}

/// Build module info from a ParsedModule tree.
///
/// This walks the module tree and:
/// 1. Registers each module in the registry
/// 2. Associates each item with its containing module
/// 3. Records module visibility for access checking
/// 4. Processes `pub use` re-exports to make re-exported items visible
pub fn build_module_info(module: &ParsedModule) -> ModuleInfo {
    let mut info = ModuleInfo::default();
    // Root module: always public, no parent
    build_module_info_recursive(module, &ModulePath::root(), None, &mut info);

    // Second pass: process pub use re-exports
    // This needs to be done after all modules are registered
    process_pub_use_reexports(module, &ModulePath::root(), &mut info);

    info
}

/// Recursive helper for building module info.
pub(super) fn build_module_info_recursive(
    module: &ParsedModule,
    current_path: &ModulePath,
    parent_path: Option<&ModulePath>,
    info: &mut ModuleInfo,
) {
    // Register this module
    info.modules.entry(current_path.clone()).or_default();

    // Record file path for this module (for multi-file error reporting)
    info.module_files
        .insert(current_path.clone(), module.path.clone());

    // Record the reverse mapping: file → canonical module path
    // Only insert if not already present (first registration wins = canonical)
    info.file_to_module
        .entry(module.path.clone())
        .or_insert_with(|| current_path.clone());

    // Record visibility: (visibility, parent_module)
    info.module_visibility.insert(
        current_path.clone(),
        (module.visibility.clone(), parent_path.cloned()),
    );

    // Process items in this module
    for item in &module.source_file.items {
        match item {
            Item::Mod(_) | Item::Error(_) => {
                // Skip mod declarations and error nodes
            }
            Item::Use(use_decl) => {
                // Track which module this use statement belongs to
                // Use (file_path, span_start) as key since span offsets can overlap across files
                info.use_statement_modules.insert(
                    (module.path.clone(), use_decl.span.start),
                    current_path.clone(),
                );
                // Also store by full span (start, end) which is more likely to be unique
                info.use_statement_by_span.insert(
                    (use_decl.span.start, use_decl.span.end),
                    current_path.clone(),
                );
            }
            Item::Function(f) => {
                let name = f.name.name.clone();
                let vis = f.visibility.clone();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.values.push(name.clone());
                    contents.value_visibility.insert(name, vis);
                }
            }
            Item::TypeDef(t) => {
                let name = t.name.name.clone();
                let vis = t.visibility.clone();
                let param_count = t.type_params.len();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.types.push(name.clone());
                    contents.type_visibility.insert(name.clone(), vis.clone());
                    // Record type parameter count for stub creation (ADR 30.1.26.1)
                    contents.type_param_counts.insert(name.clone(), param_count);
                }
                // Also register constructors from sum types
                // Constructors inherit visibility from their parent type
                if let crate::ast::TypeBody::Sum(variants) = &t.body {
                    for variant in variants {
                        let ctor_name = variant.name.name.clone();
                        info.item_modules
                            .insert(ctor_name.clone(), current_path.clone());
                        if let Some(contents) = info.modules.get_mut(current_path) {
                            contents.constructors.push(ctor_name.clone());
                            contents
                                .constructor_visibility
                                .insert(ctor_name, vis.clone());
                        }
                    }
                }
            }
            Item::TypeAlias(t) => {
                let name = t.name.name.clone();
                let vis = t.visibility.clone();
                let param_count = t.type_params.len();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.types.push(name.clone());
                    contents.type_visibility.insert(name.clone(), vis);
                    // Record type parameter count for stub creation (ADR 30.1.26.1)
                    contents.type_param_counts.insert(name, param_count);
                }
            }
            Item::Theorem(t) | Item::Lemma(t) => {
                let name = t.name.name.clone();
                let vis = t.visibility.clone();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.values.push(name.clone());
                    contents.value_visibility.insert(name, vis);
                }
            }
            Item::Axiom(a) => {
                let name = a.name.name.clone();
                let vis = a.visibility.clone();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.values.push(name.clone());
                    contents.value_visibility.insert(name, vis);
                }
            }
            Item::ExternFn(e) => {
                let name = e.name.name.clone();
                let vis = e.visibility.clone();
                info.item_modules.insert(name.clone(), current_path.clone());
                if let Some(contents) = info.modules.get_mut(current_path) {
                    contents.values.push(name.clone());
                    contents.value_visibility.insert(name, vis);
                }
            }
        }
    }

    // Recursively process submodules
    for submodule in &module.submodules {
        // Get module name from the path (filename without extension, or "mod" → parent dir name)
        let module_name = get_module_name(submodule);
        let child_path = current_path.child(module_name);
        build_module_info_recursive(submodule, &child_path, Some(current_path), info);
    }
}

/// Process `pub use` re-exports to make re-exported items visible in the module.
///
/// This is a second pass after all modules and items are registered, so we can
/// resolve the source modules and copy their items to the re-exporting module.
pub(super) fn process_pub_use_reexports(
    module: &ParsedModule,
    current_path: &ModulePath,
    info: &mut ModuleInfo,
) {
    // Process pub use declarations in this module
    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            // Only process `pub use` declarations
            if !matches!(use_decl.visibility, Visibility::Public | Visibility::Crate) {
                continue;
            }

            // Expand the use tree
            match use_decl.tree.expand() {
                ExpandedUseTree::Paths(paths) => {
                    for path in paths {
                        // Get module path (all but last segment) and item name (last segment)
                        if path.segments.len() >= 2 {
                            let item_name = path.segments.last().unwrap().name.clone();
                            let module_segments: Vec<String> = path.segments
                                [..path.segments.len() - 1]
                                .iter()
                                .map(|s| s.name.clone())
                                .collect();

                            // Try to resolve the source module
                            let source_module =
                                resolve_pub_use_module(&module_segments, current_path, info);

                            if let Some(src_mod) = source_module {
                                // Copy item from source module to current module
                                copy_item_to_module(&src_mod, &item_name, current_path, info);
                            }
                        }
                    }
                }
                ExpandedUseTree::Glob { prefix, .. } => {
                    // pub use foo::* - copy all items from foo
                    let module_segments: Vec<String> =
                        prefix.segments.iter().map(|s| s.name.clone()).collect();

                    let source_module =
                        resolve_pub_use_module(&module_segments, current_path, info);

                    if let Some(src_mod) = source_module {
                        // Copy all items from source module
                        copy_all_items_to_module(&src_mod, current_path, info);
                    }
                }
            }
        }
    }

    // Recursively process submodules
    for submodule in &module.submodules {
        let module_name = get_module_name(submodule);
        let child_path = current_path.child(module_name);
        process_pub_use_reexports(submodule, &child_path, info);
    }
}

/// Resolve a module path for pub use (relative to current module).
fn resolve_pub_use_module(
    segments: &[String],
    current_path: &ModulePath,
    info: &ModuleInfo,
) -> Option<ModulePath> {
    if segments.is_empty() {
        return None;
    }

    // Build the raw path
    let raw_path = ModulePath::from_segments(segments);

    // Try child resolution first (e.g., "common" in ast → ast::common)
    let child_path = current_path.join(&raw_path);
    if info.modules.contains_key(&child_path) {
        return Some(child_path);
    }

    // Try sibling resolution
    if let Some(parent) = current_path.parent() {
        let sibling_path = parent.join(&raw_path);
        if info.modules.contains_key(&sibling_path) {
            return Some(sibling_path);
        }
    }

    // Try absolute resolution
    if info.modules.contains_key(&raw_path) {
        return Some(raw_path);
    }

    None
}

/// Copy a specific item from source module to target module.
fn copy_item_to_module(
    source_module: &ModulePath,
    item_name: &str,
    target_module: &ModulePath,
    info: &mut ModuleInfo,
) {
    // Get source module contents
    let source_contents = match info.modules.get(source_module) {
        Some(c) => c.clone(),
        None => return,
    };

    // Check if item exists in source and copy to target
    let target_contents = info.modules.entry(target_module.clone()).or_default();

    if source_contents.types.iter().any(|n| n == item_name) {
        if !target_contents.types.iter().any(|n| n == item_name) {
            target_contents.types.push(item_name.to_string());
            // Copy visibility if available
            if let Some(vis) = source_contents.type_visibility.get(item_name) {
                target_contents
                    .type_visibility
                    .insert(item_name.to_string(), vis.clone());
            }
            // Copy type param count for generic types (ADR 30.1.26.1)
            if let Some(&count) = source_contents.type_param_counts.get(item_name) {
                target_contents
                    .type_param_counts
                    .insert(item_name.to_string(), count);
            }
        }
    }

    if source_contents.values.iter().any(|n| n == item_name) {
        if !target_contents.values.iter().any(|n| n == item_name) {
            target_contents.values.push(item_name.to_string());
            // Copy visibility if available
            if let Some(vis) = source_contents.value_visibility.get(item_name) {
                target_contents
                    .value_visibility
                    .insert(item_name.to_string(), vis.clone());
            }
        }
    }

    if source_contents.constructors.iter().any(|n| n == item_name) {
        if !target_contents.constructors.iter().any(|n| n == item_name) {
            target_contents.constructors.push(item_name.to_string());
            // Copy visibility if available
            if let Some(vis) = source_contents.constructor_visibility.get(item_name) {
                target_contents
                    .constructor_visibility
                    .insert(item_name.to_string(), vis.clone());
            }
        }
    }
}

/// Copy all items from source module to target module (for glob re-exports).
fn copy_all_items_to_module(
    source_module: &ModulePath,
    target_module: &ModulePath,
    info: &mut ModuleInfo,
) {
    // Get source module contents
    let source_contents = match info.modules.get(source_module) {
        Some(c) => c.clone(),
        None => return,
    };

    let target_contents = info.modules.entry(target_module.clone()).or_default();

    // Copy all types with visibility
    for name in &source_contents.types {
        if !target_contents.types.iter().any(|n| n == name) {
            target_contents.types.push(name.clone());
            if let Some(vis) = source_contents.type_visibility.get(name) {
                target_contents
                    .type_visibility
                    .insert(name.clone(), vis.clone());
            }
            // Copy type param count for generic types (ADR 30.1.26.1)
            if let Some(&count) = source_contents.type_param_counts.get(name) {
                target_contents
                    .type_param_counts
                    .insert(name.clone(), count);
            }
        }
    }

    // Copy all values with visibility
    for name in &source_contents.values {
        if !target_contents.values.iter().any(|n| n == name) {
            target_contents.values.push(name.clone());
            if let Some(vis) = source_contents.value_visibility.get(name) {
                target_contents
                    .value_visibility
                    .insert(name.clone(), vis.clone());
            }
        }
    }

    // Copy all constructors with visibility
    for name in &source_contents.constructors {
        if !target_contents.constructors.iter().any(|n| n == name) {
            target_contents.constructors.push(name.clone());
            if let Some(vis) = source_contents.constructor_visibility.get(name) {
                target_contents
                    .constructor_visibility
                    .insert(name.clone(), vis.clone());
            }
        }
    }
}

/// Extract module name from a ParsedModule.
pub(super) fn get_module_name(module: &ParsedModule) -> String {
    let file_name = module
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    if file_name == "mod" {
        // For mod.tg, use parent directory name
        module
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        file_name.to_string()
    }
}

impl ModuleInfo {
    /// Canonicalize a module path by resolving through file mapping.
    ///
    /// When the same logical module is registered under different paths
    /// (e.g., `main::parser::items::types` vs `parser::items::types`),
    /// this returns the canonical path (the first one registered).
    ///
    /// This is used to fix false visibility errors where the item_module
    /// and from_module represent the same file but have different path prefixes.
    pub fn canonicalize_path(&self, path: &ModulePath) -> ModulePath {
        // Look up the file for this module path
        if let Some(file) = self.module_files.get(path) {
            // Look up the canonical module path for this file
            if let Some(canonical) = self.file_to_module.get(file) {
                return canonical.clone();
            }
        }
        // No mapping found - return original path
        path.clone()
    }
}
