//! Module registry methods for Env.
//!
//! Manages the module system: registration, lookup, population, and
//! use-statement-to-module resolution.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::Visibility;

use crate::elaborate::env::{ConstructorInfo, Env, TypeDef, TypeDefKind};

use super::contents::ModuleContents;
use super::path::ModulePath;

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Module registry
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a module in the module registry.
    pub fn register_module(&mut self, path: ModulePath) {
        self.modules.entry(path).or_default();
    }

    /// Register that an item belongs to a module.
    pub fn set_item_module(&mut self, item_name: &str, module: ModulePath) {
        self.item_modules
            .insert(item_name.to_string(), module.clone());

        // Also add to module contents
        if let Some(contents) = self.modules.get_mut(&module) {
            if !contents.values.contains(&item_name.to_string())
                && !contents.types.contains(&item_name.to_string())
                && !contents.constructors.contains(&item_name.to_string())
            {
                // We'll categorize properly when the item is defined
            }
        }
    }

    /// Look up which module an item belongs to.
    pub fn get_item_module(&self, item_name: &str) -> Option<&ModulePath> {
        self.item_modules.get(item_name)
    }

    /// Check if a module exists.
    pub fn has_module(&self, path: &ModulePath) -> bool {
        self.modules.contains_key(path)
    }

    /// Get module contents.
    pub fn get_module(&self, path: &ModulePath) -> Option<&ModuleContents> {
        self.modules.get(path)
    }

    /// Get all registered module paths (for suggestions in error messages).
    pub fn all_module_paths(&self) -> impl Iterator<Item = &ModulePath> {
        self.modules.keys()
    }

    /// Populate module info in bulk (used when initializing from driver).
    ///
    /// This also creates stub type definitions for all types in the module info,
    /// allowing them to be resolved during type checking. The stubs will be
    /// replaced by actual definitions when the file containing them is elaborated.
    pub fn populate_module_info(&mut self, module_info: crate::driver::modules::ModuleInfo) {
        let crate::driver::modules::ModuleInfo {
            modules,
            item_modules,
            module_visibility,
            use_statement_modules,
            use_statement_by_span,
            item_index_to_file,
            module_files,
            file_to_module,
        } = module_info;
        // Create stub type definitions for all types in module contents.
        // This allows types from workspace sibling modules to be resolved.
        // Sort modules by path to ensure deterministic item_modules registration
        // when the same item appears in multiple modules (e.g., `helper` and
        // `main::helper`). Shorter paths sort first, so the defining module wins
        // over re-export paths.
        let mut sorted_modules: Vec<_> = modules.iter().collect();
        sorted_modules.sort_by_key(|(path, _)| path.to_string());
        for (module_path, contents) in sorted_modules {
            for type_name in &contents.types {
                // Only create stub if not already registered
                if !self.types.contains_key(type_name) {
                    // Get the type parameter count if available (ADR 30.1.26.1)
                    // This preserves arity information for generic types like List<T>
                    let param_count = contents
                        .type_param_counts
                        .get(type_name)
                        .copied()
                        .unwrap_or(0);
                    let params: Vec<String> = (0..param_count).map(|i| format!("T{}", i)).collect();

                    self.types.insert(
                        type_name.clone(),
                        TypeDef {
                            name: type_name.clone(),
                            params,
                            kind: TypeDefKind::Stub,
                            visibility: Visibility::Public, // Assume public
                            span: crate::span::Span::new(0, 0),
                            // ADR 31: Track canonical defining module for stubs
                            defining_module: Some(module_path.clone()),
                            encoded_type: None,
                            field_visibilities: Vec::new(),
                        },
                    );
                    // Track which module this type is from
                    self.item_modules
                        .insert(type_name.clone(), module_path.clone());
                }
            }
            // Register constructor stubs from pre-parsed ADT details (ADR 5.5.26b).
            // This ensures cross-branch imports can resolve constructors even before
            // the defining module is elaborated.
            for (ctor_name, detail) in &contents.constructor_details {
                self.constructors
                    .entry(ctor_name.clone())
                    .or_insert(ConstructorInfo {
                        type_name: detail.type_name.clone(),
                        index: detail.index,
                        arity: detail.arity,
                        visibility: None,
                        defining_module: Some(module_path.clone()),
                    });
            }
        }

        self.modules = modules;
        // Merge item_modules - don't overwrite since we just set them
        for (name, module) in item_modules {
            self.item_modules.entry(name).or_insert(module);
        }
        self.module_visibility = module_visibility;
        self.use_statement_modules = use_statement_modules;
        self.use_statement_by_span = use_statement_by_span;
        self.item_index_to_file = item_index_to_file;
        self.module_files = module_files;
        self.file_to_module = file_to_module;
    }

    /// Get the file path for an item by its index in the combined AST.
    pub fn get_item_file(&self, index: usize) -> Option<&PathBuf> {
        self.item_index_to_file.get(index)
    }

    /// Get the module that a use statement belongs to, given its file and span.
    ///
    /// This is the primary lookup mechanism for use statement module resolution.
    /// The file_path disambiguates when different files have items at same byte offsets.
    pub fn get_use_statement_module_by_file(
        &self,
        file_path: &PathBuf,
        span_start: u32,
    ) -> Option<&ModulePath> {
        self.use_statement_modules
            .get(&(file_path.clone(), span_start))
    }

    /// Legacy: Get the module that a use statement belongs to, based on its span only.
    ///
    /// Falls back to span-only lookups. May have collisions if different files
    /// have items at the same byte offsets.
    pub fn get_use_statement_module(&self, span_start: u32, span_end: u32) -> Option<&ModulePath> {
        // Fallback: try the full span lookup (may have collisions across files)
        if let Some(module) = self.use_statement_by_span.get(&(span_start, span_end)) {
            return Some(module);
        }

        // Last resort: start-only lookup across all files (ambiguous, picks first)
        let mut matches: Vec<_> = self
            .use_statement_modules
            .iter()
            .filter(|((_, start), _)| *start == span_start)
            .collect();

        // Sort by file path for deterministic behavior
        matches.sort_by_key(|((path, _), _)| path.as_os_str());

        matches.first().map(|(_, module)| *module)
    }

    /// Get the file path for a module (for error reporting).
    pub fn get_module_file(&self, module: &ModulePath) -> Option<&PathBuf> {
        self.module_files.get(module)
    }

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
