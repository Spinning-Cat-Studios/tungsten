//! Module info extraction for the elaborator.
//!
//! This module builds the `ModuleInfo` structure that the elaborator uses
//! to resolve items across modules.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::{Item, Visibility};
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
    super::info_reexports::process_pub_use_reexports(module, &ModulePath::root(), &mut info);

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
                register_use_statement(info, &module.path, use_decl, current_path);
            }
            Item::Function(f) => {
                register_value_item(info, &f.name.name, &f.visibility, current_path);
            }
            Item::TypeDef(t) => {
                register_type_def_item(info, t, current_path);
            }
            Item::TypeAlias(t) => {
                register_type_alias_item(info, t, current_path);
            }
            Item::Theorem(t) | Item::Lemma(t) => {
                register_value_item(info, &t.name.name, &t.visibility, current_path);
            }
            Item::Axiom(a) => {
                register_value_item(info, &a.name.name, &a.visibility, current_path);
            }
            Item::ExternFn(e) => {
                register_value_item(info, &e.name.name, &e.visibility, current_path);
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

/// Register a `use` declaration's module mapping.
fn register_use_statement(
    info: &mut ModuleInfo,
    file_path: &PathBuf,
    use_decl: &crate::ast::UseDecl,
    current_path: &ModulePath,
) {
    info.use_statement_modules.insert(
        (file_path.clone(), use_decl.span.start),
        current_path.clone(),
    );
    info.use_statement_by_span.insert(
        (use_decl.span.start, use_decl.span.end),
        current_path.clone(),
    );
}

/// Register a value-producing item (function, theorem, lemma, axiom, extern fn).
#[allow(clippy::trivially_copy_pass_by_ref)] // Reason: &Visibility matches the pattern of other register functions
fn register_value_item(
    info: &mut ModuleInfo,
    name: &str,
    vis: &Visibility,
    current_path: &ModulePath,
) {
    info.item_modules
        .insert(name.to_string(), current_path.clone());
    if let Some(contents) = info.modules.get_mut(current_path) {
        contents.values.push(name.to_string());
        contents
            .value_visibility
            .insert(name.to_string(), vis.clone());
    }
}

/// Register a type definition (sum type / record), including constructors.
fn register_type_def_item(
    info: &mut ModuleInfo,
    t: &crate::ast::TypeDef,
    current_path: &ModulePath,
) {
    let name = t.name.name.clone();
    let vis = t.visibility.clone();
    let param_count = t.type_params.len();
    info.item_modules.insert(name.clone(), current_path.clone());
    if let Some(contents) = info.modules.get_mut(current_path) {
        contents.types.push(name.clone());
        contents.type_visibility.insert(name.clone(), vis.clone());
        contents.type_param_counts.insert(name.clone(), param_count);
    }
    // Also register constructors from sum types
    if let crate::ast::TypeBody::Sum(variants) = &t.body {
        for (index, variant) in variants.iter().enumerate() {
            let ctor_name = variant.name.name.clone();
            let arity = variant.fields.len();
            info.item_modules
                .insert(ctor_name.clone(), current_path.clone());
            if let Some(contents) = info.modules.get_mut(current_path) {
                contents.constructors.push(ctor_name.clone());
                contents
                    .constructor_visibility
                    .insert(ctor_name.clone(), vis.clone());
                // Store details for constructor stub creation (ADR 5.5.26b)
                contents.constructor_details.insert(
                    ctor_name,
                    crate::elaborate::ConstructorStubDetail {
                        type_name: name.clone(),
                        index,
                        arity,
                    },
                );
            }
        }
    }
}

/// Register a type alias item.
fn register_type_alias_item(
    info: &mut ModuleInfo,
    t: &crate::ast::TypeAlias,
    current_path: &ModulePath,
) {
    let name = t.name.name.clone();
    let vis = t.visibility.clone();
    let param_count = t.type_params.len();
    info.item_modules.insert(name.clone(), current_path.clone());
    if let Some(contents) = info.modules.get_mut(current_path) {
        contents.types.push(name.clone());
        contents.type_visibility.insert(name.clone(), vis);
        contents.type_param_counts.insert(name, param_count);
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
