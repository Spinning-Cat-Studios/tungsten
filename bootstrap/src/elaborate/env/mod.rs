//! Name resolution environment for the elaborator.
//!
//! The environment tracks:
//! - Type definitions (ADTs, type aliases)
//! - Value definitions (functions, theorems, axioms)
//! - Local variable bindings (in scope during elaboration)
//! - Module contents (for qualified path resolution)
//! - Imports (for `use` statements)
//!
//! This module is split into submodules for organization:
//! - `modules`: Module path, contents, and registry management
//! - `definitions`: Type, value, and constructor definitions
//! - `resolution`: Path resolution, canonical lookup, and visibility
//! - `imports`: Import management and lookups

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::Visibility;
use tungsten_core::Type;

// Submodules
mod definitions;
mod imports;
mod locals;
mod modules;
mod queries;
mod resolution;
mod type_defs;

#[cfg(test)]
mod tests;

// Re-export all public types
pub use definitions::{
    Constructor, ConstructorInfo, LocalBinding, ResolvedValue, TypeDef, TypeDefKind, ValueDef,
};
pub use imports::{ImportInfo, ImportRequest};
pub use modules::{ConstructorStubDetail, ModuleContents, ModulePath, PathResolutionError};
pub use resolution::CanonicalResolutionError;

/// Environment for name resolution during elaboration.
///
/// Tracks all definitions in scope and provides lookup methods.
#[derive(Debug, Clone)]
pub struct Env {
    /// Type definitions: `type Name<T> = ...` or ADT
    pub(crate) types: HashMap<String, TypeDef>,

    /// Value definitions: fn, theorem, let bindings
    pub(crate) values: HashMap<String, ValueDef>,

    /// Constructor → Parent type mapping
    pub(crate) constructors: HashMap<String, ConstructorInfo>,

    /// Scoped local variables (stack of scopes)
    /// Each scope is a map from name to binding
    locals: Vec<HashMap<String, LocalBinding>>,

    /// Type variables currently in scope
    type_vars: Vec<String>,

    // ─────────────────────────────────────────────────────────────────────────
    // Module system (Phase 3)
    // ─────────────────────────────────────────────────────────────────────────
    /// Module registry: maps module paths to their contents
    pub(crate) modules: HashMap<ModulePath, ModuleContents>,

    /// Item to module mapping: which module each item belongs to
    item_modules: HashMap<String, ModulePath>,

    /// Imported types: local name → import info
    pub(crate) imported_types: HashMap<String, imports::ImportInfo>,

    /// Imported values: local name → import info
    pub(crate) imported_values: HashMap<String, imports::ImportInfo>,

    /// Imported constructors: local name → import info
    pub(crate) imported_constructors: HashMap<String, imports::ImportInfo>,

    /// Module visibility: maps module paths to their visibility and declaring parent
    /// The tuple is (visibility, parent_module) where parent_module is the module
    /// that declared this module (None for root)
    pub(crate) module_visibility: HashMap<ModulePath, (Visibility, Option<ModulePath>)>,

    /// Use statement to module mapping: (file_path, span_start) → module path
    /// Used to determine which module a use statement belongs to when processing imports.
    /// The file_path is needed because span offsets can overlap across files.
    use_statement_modules: HashMap<(PathBuf, u32), ModulePath>,

    /// Alternative mapping: (span_start, span_end) → module path
    /// Full spans are more likely to be unique across files
    use_statement_by_span: HashMap<(u32, u32), ModulePath>,

    /// Item index to file path mapping: index in combined AST → file path
    /// Used to determine which file an item came from in the combined AST.
    /// Unlike span-based mapping, indices are always unique.
    item_index_to_file: Vec<PathBuf>,

    /// Module to file path mapping: module path → source file path
    /// Used for multi-file error reporting
    pub(crate) module_files: HashMap<ModulePath, PathBuf>,

    /// File to canonical module path mapping: source file → canonical module path
    /// Used for path canonicalization to resolve path prefix mismatches.
    file_to_module: HashMap<PathBuf, ModulePath>,

    /// When true, trace constructor registration calls (--trace-constructor-registration, ADR 7.5.26e).
    pub(crate) trace_ctor_registration: bool,
}

impl Env {
    /// Create a new empty environment.
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
            values: HashMap::new(),
            constructors: HashMap::new(),
            locals: Vec::new(),
            type_vars: Vec::new(),
            modules: HashMap::new(),
            item_modules: HashMap::new(),
            imported_types: HashMap::new(),
            imported_values: HashMap::new(),
            imported_constructors: HashMap::new(),
            module_visibility: HashMap::new(),
            use_statement_modules: HashMap::new(),
            use_statement_by_span: HashMap::new(),
            item_index_to_file: Vec::new(),
            module_files: HashMap::new(),
            file_to_module: HashMap::new(),
            trace_ctor_registration: false,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Value definitions
    // ─────────────────────────────────────────────────────────────────────────

    /// Define a new value (function, theorem, etc.) with module context.
    pub fn define_value_in_module(&mut self, def: ValueDef, module: ModulePath) {
        let name = def.name.clone();

        // Track value's module
        self.item_modules.insert(name.clone(), module.clone());
        if let Some(contents) = self.modules.get_mut(&module) {
            contents.values.push(name.clone());
        }

        self.values.insert(name, def);
    }

    /// Define a new value (function, theorem, etc.).
    pub fn define_value(&mut self, def: ValueDef) {
        self.values.insert(def.name.clone(), def);
    }

    /// Look up a value definition by name.
    pub fn lookup_value(&self, name: &str) -> Option<&ValueDef> {
        self.values.get(name)
    }

    /// Iterate over all value definitions.
    pub fn iter_values(&self) -> impl Iterator<Item = (&String, &ValueDef)> {
        self.values.iter()
    }

    /// Check if a value name is defined.
    pub fn has_value(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Constructors
    // ─────────────────────────────────────────────────────────────────────────

    /// Look up a constructor by name.
    pub fn lookup_constructor(&self, name: &str) -> Option<&ConstructorInfo> {
        self.constructors.get(name)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // General resolution
    // ─────────────────────────────────────────────────────────────────────────

    /// Resolve a value name, checking locals first, then globals, then constructors.
    ///
    /// Returns the resolution with type information.
    pub fn resolve_value(&self, name: &str, current_depth: usize) -> Option<ResolvedValue> {
        // 1. Check local variables (innermost scope first)
        if let Some(binding) = self.lookup_local(name) {
            // Calculate de Bruijn index: current_depth - level_at_binding - 1
            let index = current_depth - binding.level - 1;
            return Some(ResolvedValue::Local(index, binding.ty.clone()));
        }

        // 2. Check global values
        if let Some(def) = self.values.get(name) {
            return Some(ResolvedValue::Global(def.name.clone(), def.ty.clone()));
        }

        // 3. Check constructors
        if let Some(info) = self.constructors.get(name) {
            return Some(ResolvedValue::Constructor(info.clone()));
        }

        None
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
