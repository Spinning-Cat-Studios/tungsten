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
//! - `module_path`: Module path type and utilities
//! - `module_contents`: Module contents and path resolution errors
//! - `definitions`: Type, value, and constructor definitions
//! - `visibility`: Visibility checking logic
//! - `resolution`: Path resolution for types, values, and constructors
//! - `imports`: Import management

use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::Visibility;
use tungsten_core::Type;

// Submodules
mod definitions;
mod imports;
mod module_contents;
mod module_path;
mod resolution;
mod visibility;

// Re-export all public types
pub use definitions::{
    Constructor, ConstructorInfo, LocalBinding, ResolvedValue, TypeDef, TypeDefKind, ValueDef,
};
pub use imports::ImportInfo;
pub use module_contents::{ModuleContents, PathResolutionError};
pub use module_path::ModulePath;

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
        }
    }

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
    pub fn populate_module_info(
        &mut self,
        modules: HashMap<ModulePath, ModuleContents>,
        item_modules: HashMap<String, ModulePath>,
        module_visibility: HashMap<ModulePath, (Visibility, Option<ModulePath>)>,
        use_statement_modules: HashMap<(PathBuf, u32), ModulePath>,
        use_statement_by_span: HashMap<(u32, u32), ModulePath>,
        item_index_to_file: Vec<PathBuf>,
        module_files: HashMap<ModulePath, PathBuf>,
        file_to_module: HashMap<PathBuf, ModulePath>,
    ) {
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
                        },
                    );
                    // Track which module this type is from
                    self.item_modules
                        .insert(type_name.clone(), module_path.clone());
                }
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

    // ─────────────────────────────────────────────────────────────────────────
    // Type definitions
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a type name as a stub (for Phase 1a).
    ///
    /// This makes the type name available for import resolution before
    /// the type body is fully elaborated. The stub will be replaced
    /// by the full definition in Phase 1c.
    ///
    /// The `params` argument captures the type parameter names so that
    /// forward references to generic types (e.g., `Forest<T>` referenced
    /// before `Forest` is elaborated) have the correct arity.
    pub fn register_type_stub(
        &mut self,
        name: &str,
        params: Vec<String>,
        visibility: crate::ast::Visibility,
        span: crate::span::Span,
    ) {
        // Create a placeholder type definition with correct arity
        // The kind doesn't matter since it will be replaced
        let stub = TypeDef {
            name: name.to_string(),
            params,
            kind: TypeDefKind::Stub,
            visibility,
            span,
            defining_module: None, // Local stub, will be replaced with real def
            encoded_type: None,
        };
        self.types.insert(name.to_string(), stub);
    }

    /// Define a new type (with optional module context).
    pub fn define_type_in_module(&mut self, def: TypeDef, module: ModulePath) {
        let name = def.name.clone();

        // Register constructors if this is an ADT
        if let TypeDefKind::ADT(ref ctors) = def.kind {
            for ctor in ctors {
                self.constructors.insert(
                    ctor.name.clone(),
                    ConstructorInfo {
                        type_name: def.name.clone(),
                        index: ctor.index,
                        arity: ctor.fields.len(),
                        defining_module: Some(module.clone()),
                    },
                );
                // Track constructor's module
                self.item_modules.insert(ctor.name.clone(), module.clone());
                if let Some(contents) = self.modules.get_mut(&module) {
                    contents.constructors.push(ctor.name.clone());
                }
            }
        }

        // Track type's module
        self.item_modules.insert(name.clone(), module.clone());
        if let Some(contents) = self.modules.get_mut(&module) {
            contents.types.push(name.clone());
        }

        self.types.insert(name, def);
    }

    /// Define a new type.
    pub fn define_type(&mut self, def: TypeDef) {
        // Register constructors if this is an ADT
        if let TypeDefKind::ADT(ref ctors) = def.kind {
            for ctor in ctors {
                self.constructors.insert(
                    ctor.name.clone(),
                    ConstructorInfo {
                        type_name: def.name.clone(),
                        index: ctor.index,
                        arity: ctor.fields.len(),
                        defining_module: None, // No module context in simple define_type
                    },
                );
            }
        }
        self.types.insert(def.name.clone(), def);
    }

    /// Look up a type definition by name.
    pub fn lookup_type(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(name)
    }

    /// Look up a type definition by name, following canonical module references.
    ///
    /// This is the ADR 31 canonical lookup that handles cross-module generic types.
    /// If the type is a stub with a defining_module, looks up the real definition
    /// from that module's types.
    ///
    /// For pattern matching on imported ADTs, use this instead of `lookup_type`.
    pub fn lookup_type_canonical(&self, name: &str) -> Option<&TypeDef> {
        let typedef = self.types.get(name)?;

        // If this is a stub with a canonical defining module, try to find the real def
        if matches!(typedef.kind, TypeDefKind::Stub) {
            if let Some(ref defining_module) = typedef.defining_module {
                // Look in the defining module's contents for the real type
                if let Some(contents) = self.modules.get(defining_module) {
                    // Check if the module actually defines this type (not just imports it)
                    if contents.types.contains(&name.to_string())
                        && !contents.imported_types.contains_key(name)
                    {
                        // The type should be in the global types map under the same name
                        // but might have been elaborated by now
                        if let Some(real_def) = self.types.get(name) {
                            if !matches!(real_def.kind, TypeDefKind::Stub) {
                                return Some(real_def);
                            }
                        }
                    }
                }
            }
        }

        Some(typedef)
    }

    /// Look up a type name by its encoded representation.
    ///
    /// This enables reverse lookup from Core types to user-defined type names
    /// for better error messages. Only works for non-parameterized types that
    /// have their `encoded_type` cached.
    pub fn lookup_type_name_by_encoding(&self, encoded: &Type) -> Option<&str> {
        for (name, def) in &self.types {
            if let Some(ref cached) = def.encoded_type {
                if cached == encoded {
                    return Some(name);
                }
            }
        }
        None
    }

    /// Iterate over all type definitions.
    pub fn iter_types(&self) -> impl Iterator<Item = (&String, &TypeDef)> {
        self.types.iter()
    }

    /// Check if a type name is defined.
    pub fn has_type(&self, name: &str) -> bool {
        self.types.contains_key(name)
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
    // Local scopes
    // ─────────────────────────────────────────────────────────────────────────

    /// Enter a new local scope.
    pub fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }

    /// Exit the current local scope.
    pub fn pop_scope(&mut self) {
        self.locals.pop();
    }

    /// Bind a local variable in the current scope.
    pub fn bind_local(&mut self, name: String, ty: Type, level: usize) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.clone(), LocalBinding { name, ty, level });
        }
    }

    /// Look up a local variable by name.
    fn lookup_local(&self, name: &str) -> Option<&LocalBinding> {
        // Search from innermost to outermost scope
        for scope in self.locals.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding);
            }
        }
        None
    }

    /// Get the current de Bruijn depth (number of local bindings).
    #[allow(dead_code)]
    pub fn depth(&self) -> usize {
        self.locals.iter().map(|s| s.len()).sum()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Type variables
    // ─────────────────────────────────────────────────────────────────────────

    /// Add a type variable to scope.
    pub fn push_type_var(&mut self, name: String) {
        self.type_vars.push(name);
    }

    /// Remove a type variable from scope.
    pub fn pop_type_var(&mut self) {
        self.type_vars.pop();
    }

    /// Check if a type variable is in scope.
    pub fn has_type_var(&self, name: &str) -> bool {
        self.type_vars.contains(&name.to_string())
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

    // ─────────────────────────────────────────────────────────────────────────
    // Name suggestions (for "did you mean" errors)
    // ─────────────────────────────────────────────────────────────────────────

    /// Get all type names in scope (for "did you mean" suggestions).
    ///
    /// Includes:
    /// - Defined types (ADTs, type aliases)
    /// - Type variables in scope
    /// - Built-in types
    pub fn all_type_names(&self) -> impl Iterator<Item = &str> {
        // Built-in types
        let builtins = ["Nat", "Bool", "Unit", "String", "Eq", "Void"].into_iter();

        // Defined types
        let defined = self.types.keys().map(|s| s.as_str());

        // Type variables
        let type_vars = self.type_vars.iter().map(|s| s.as_str());

        builtins.chain(defined).chain(type_vars)
    }

    /// Get all value names in scope (for "did you mean" suggestions).
    ///
    /// Includes:
    /// - Global functions and theorems
    /// - Constructors
    /// - Local variables
    pub fn all_value_names(&self) -> impl Iterator<Item = &str> {
        // Global values
        let globals = self.values.keys().map(|s| s.as_str());

        // Constructors
        let constructors = self.constructors.keys().map(|s| s.as_str());

        // Local variables from all scopes
        let locals = self
            .locals
            .iter()
            .flat_map(|scope| scope.keys().map(|s| s.as_str()));

        globals.chain(constructors).chain(locals)
    }

    /// Get all constructor names in scope (for "did you mean" suggestions).
    ///
    /// This is a subset of `all_value_names()`, but dedicated for constructor-specific
    /// error messages where we only want to suggest other constructors.
    pub fn all_constructor_names(&self) -> impl Iterator<Item = &str> {
        self.constructors.keys().map(|s| s.as_str())
    }

    /// Export all type definitions for hashing (used by IR cache).
    ///
    /// Returns a sorted vector of (name, typedef) pairs for deterministic hashing.
    pub fn export_types_for_hash(&self) -> Vec<(String, TypeDef)> {
        let mut types: Vec<_> = self
            .types
            .iter()
            .map(|(name, def)| (name.clone(), def.clone()))
            .collect();
        types.sort_by(|(a, _), (b, _)| a.cmp(b));
        types
    }

    /// Export all value signatures for hashing (used by IR cache).
    ///
    /// Returns a sorted vector of (name, type) pairs for deterministic hashing.
    pub fn export_value_signatures_for_hash(&self) -> Vec<(String, Type)> {
        let mut values: Vec<_> = self
            .values
            .iter()
            .map(|(name, def)| (name.clone(), def.ty.clone()))
            .collect();
        values.sort_by(|(a, _), (b, _)| a.cmp(b));
        values
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn test_env_new() {
        let env = Env::new();
        assert!(env.types.is_empty());
        assert!(env.values.is_empty());
        assert!(env.constructors.is_empty());
    }

    #[test]
    fn test_define_value() {
        let mut env = Env::new();
        env.define_value(ValueDef {
            name: "foo".to_string(),
            ty: Type::Nat,
            visibility: Visibility::Private,
            span: Span::new(0, 3),
        });

        assert!(env.has_value("foo"));
        assert!(!env.has_value("bar"));

        let def = env.lookup_value("foo").unwrap();
        assert_eq!(def.name, "foo");
        assert_eq!(def.ty, Type::Nat);
    }

    #[test]
    fn test_local_scopes() {
        let mut env = Env::new();

        // Enter scope and bind x
        env.push_scope();
        env.bind_local("x".to_string(), Type::Nat, 0);

        // Can resolve x
        assert!(env.lookup_local("x").is_some());

        // Enter nested scope and bind y
        env.push_scope();
        env.bind_local("y".to_string(), Type::Bool, 1);

        // Can resolve both
        assert!(env.lookup_local("x").is_some());
        assert!(env.lookup_local("y").is_some());

        // Exit nested scope
        env.pop_scope();

        // y is gone, x remains
        assert!(env.lookup_local("x").is_some());
        assert!(env.lookup_local("y").is_none());

        // Exit outer scope
        env.pop_scope();
        assert!(env.lookup_local("x").is_none());
    }

    #[test]
    fn test_type_vars() {
        let mut env = Env::new();

        assert!(!env.has_type_var("T"));

        env.push_type_var("T".to_string());
        assert!(env.has_type_var("T"));

        env.push_type_var("U".to_string());
        assert!(env.has_type_var("T"));
        assert!(env.has_type_var("U"));

        env.pop_type_var();
        assert!(env.has_type_var("T"));
        assert!(!env.has_type_var("U"));
    }

    #[test]
    fn test_resolve_value_local() {
        let mut env = Env::new();
        env.push_scope();
        env.bind_local("x".to_string(), Type::Nat, 0);

        let resolved = env.resolve_value("x", 1);
        assert!(matches!(resolved, Some(ResolvedValue::Local(0, Type::Nat))));
    }

    #[test]
    fn test_resolve_value_global() {
        let mut env = Env::new();
        env.define_value(ValueDef {
            name: "foo".to_string(),
            ty: Type::Nat,
            visibility: Visibility::Private,
            span: Span::new(0, 3),
        });

        let resolved = env.resolve_value("foo", 0);
        assert!(matches!(
            resolved,
            Some(ResolvedValue::Global(_, Type::Nat))
        ));
    }
}
