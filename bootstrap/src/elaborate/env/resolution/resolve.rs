//! Path resolution logic for types, values, and constructors.

use crate::ast::Path;

use crate::elaborate::env::{
    ConstructorInfo, Env, ModulePath, PathResolutionError, ResolvedValue, TypeDef, ValueDef,
};

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Qualified path resolution
    // ─────────────────────────────────────────────────────────────────────────

    /// Resolve a qualified path to a value.
    ///
    /// For simple names (single segment), this behaves like `resolve_value`.
    /// For qualified names (multiple segments), this looks up the item
    /// in the specified module.
    ///
    /// Returns: Ok(Some(resolved)) on success, Ok(None) if not found,
    /// Err((module_path, item_name)) if module not found.
    pub fn resolve_value_path(
        &self,
        path: &Path,
        current_depth: usize,
        current_module: &ModulePath,
    ) -> Result<Option<ResolvedValue>, PathResolutionError> {
        if path.is_simple() {
            // Simple name - use existing resolution (includes locals and imports)
            let name = &path.item_name().name;

            // Check module-scoped imports first
            if let Some(import_info) = self.lookup_value_import(current_module, name) {
                return self
                    .resolve_in_module(&import_info.source_module, &import_info.original_name);
            }

            // Check for imported constructor
            if let Some(import_info) = self.lookup_constructor_import(current_module, name) {
                return self
                    .resolve_in_module(&import_info.source_module, &import_info.original_name);
            }

            // If this name was aliased away (imported under a different name),
            // don't fall through to the global table — the alias replaces, not supplements.
            if self.is_name_aliased_away(current_module, name) {
                return Ok(None);
            }

            // Flat global table fallback — bypasses module-scoped import visibility.
            // All definitions from all modules are in `self.values` (populated during Phase A.5).
            // This means unqualified names resolve globally unless suppressed above.
            Ok(self.resolve_value(name, current_depth))
        } else {
            // Qualified path - could be Type::Constructor or module::item
            let item_name = &path.item_name().name;

            // For two-segment paths like "MyType::A", first check if the first segment
            // is a type name (imported or local) - this handles Type::Constructor syntax
            if path.segments.len() == 2 {
                let first_segment = &path.segments[0].name;

                // Check if this is an imported type - if so, look for constructor in source module
                if let Some(import_info) = self.lookup_type_import(current_module, first_segment) {
                    // The type is imported - look up constructor in its source module
                    return self.resolve_in_module(&import_info.source_module, item_name);
                }

                // Check if it's a local type - look for constructor belonging to that type
                if let Some(ctor_info) = self.lookup_constructor(item_name) {
                    if ctor_info.type_name == *first_segment {
                        return Ok(Some(ResolvedValue::Constructor(ctor_info.clone())));
                    }
                }
            }

            // Fall back to module path resolution
            // Try relative resolution first (child module of current_module),
            // then fall back to absolute path
            let module_segments: Vec<String> = path
                .module_segments()
                .iter()
                .map(|s| s.name.clone())
                .collect();

            let raw_path = ModulePath::from_segments(&module_segments);

            // Try as child module of current_module
            let child_path = current_module.join(&raw_path);
            if self.has_module(&child_path) {
                return self.resolve_in_module(&child_path, item_name);
            }

            // Try as sibling module (relative to parent)
            if let Some(parent) = current_module.parent() {
                let sibling_path = parent.join(&raw_path);
                if self.has_module(&sibling_path) {
                    return self.resolve_in_module(&sibling_path, item_name);
                }
            }

            // Fall back to absolute path
            self.resolve_in_module(&raw_path, item_name)
        }
    }

    /// Resolve an item in a specific module.
    fn resolve_in_module(
        &self,
        module_path: &ModulePath,
        item_name: &str,
    ) -> Result<Option<ResolvedValue>, PathResolutionError> {
        // Check if module exists
        if !self.has_module(module_path) {
            return Err(PathResolutionError::ModuleNotFound(module_path.clone()));
        }

        // First, check if item is listed in module contents
        // This is more reliable than item_modules which can have collisions
        let has_ctor = self.has_constructor_in_module(module_path, item_name);
        let has_value = self.has_value_in_module(module_path, item_name);

        if has_ctor {
            if let Some(info) = self.lookup_constructor(item_name) {
                return Ok(Some(ResolvedValue::Constructor(info.clone())));
            }
        }

        if has_value {
            if let Some(def) = self.lookup_value(item_name) {
                return Ok(Some(ResolvedValue::Global(
                    def.name.clone(),
                    def.ty.clone(),
                )));
            }
        }

        // Fallback: Check if item_modules says it's in this module (handles items not yet in module contents)
        if let Some(item_module) = self.get_item_module(item_name) {
            if item_module == module_path {
                // Item exists and is in the right module - look it up
                if let Some(def) = self.lookup_value(item_name) {
                    return Ok(Some(ResolvedValue::Global(
                        def.name.clone(),
                        def.ty.clone(),
                    )));
                }
                if let Some(info) = self.lookup_constructor(item_name) {
                    return Ok(Some(ResolvedValue::Constructor(info.clone())));
                }
            }
        }

        // Item not found in module
        Ok(None)
    }

    /// Resolve a qualified path to a type.
    ///
    /// For simple names (single segment), checks imports then local types.
    /// For qualified names (multiple segments), looks up the type in the specified module.
    pub fn resolve_type_path(
        &self,
        path: &Path,
        current_module: &ModulePath,
    ) -> Result<Option<&TypeDef>, PathResolutionError> {
        if path.is_simple() {
            let name = &path.item_name().name;

            // Check module-scoped imports first
            if let Some(import_info) = self.lookup_type_import(current_module, name) {
                return self.resolve_type_in_module(
                    &import_info.source_module,
                    &import_info.original_name,
                );
            }

            // If this name was aliased away, don't fall through to the global table.
            if self.is_name_aliased_away(current_module, name) {
                return Ok(None);
            }

            // Then direct lookup
            Ok(self.lookup_type(name))
        } else {
            // Qualified path - look up in the specified module
            // Try relative resolution first (child or sibling), then absolute
            let module_segments: Vec<String> = path
                .module_segments()
                .iter()
                .map(|s| s.name.clone())
                .collect();
            let item_name = &path.item_name().name;

            let raw_path = ModulePath::from_segments(&module_segments);

            // Try as child module of current_module
            let child_path = current_module.join(&raw_path);
            if self.has_module(&child_path) {
                return self.resolve_type_in_module(&child_path, item_name);
            }

            // Try as sibling module (relative to parent)
            if let Some(parent) = current_module.parent() {
                let sibling_path = parent.join(&raw_path);
                if self.has_module(&sibling_path) {
                    return self.resolve_type_in_module(&sibling_path, item_name);
                }
            }

            // Fall back to absolute path
            self.resolve_type_in_module(&raw_path, item_name)
        }
    }

    /// Resolve a type in a specific module.
    fn resolve_type_in_module(
        &self,
        module_path: &ModulePath,
        item_name: &str,
    ) -> Result<Option<&TypeDef>, PathResolutionError> {
        // Check if module exists
        if !self.has_module(module_path) {
            return Err(PathResolutionError::ModuleNotFound(module_path.clone()));
        }

        // Check if item is listed in that module's contents
        // This is more reliable than checking item_modules because item_modules
        // can have collisions when multiple modules define types with the same name.
        let has_type = self.has_type_in_module(module_path, item_name);
        if has_type {
            return Ok(self.lookup_type(item_name));
        }

        // Fallback: Check if item_modules says it's in this module
        // This handles items not yet in module contents (shouldn't happen normally)
        if let Some(item_module) = self.get_item_module(item_name) {
            if item_module == module_path {
                return Ok(self.lookup_type(item_name));
            }
        }

        Ok(None)
    }

    /// Resolve a qualified path to a constructor.
    ///
    /// For simple paths (just a constructor name), checks imports then local constructors.
    /// For qualified paths (Type::Constructor), looks up the type and then finds the constructor.
    /// Also supports module-qualified paths (module::Constructor).
    pub fn resolve_constructor_path(
        &self,
        path: &Path,
        current_module: &ModulePath,
    ) -> Result<Option<&ConstructorInfo>, PathResolutionError> {
        if path.is_simple() {
            let name = &path.item_name().name;

            // Check module-scoped imports first
            if let Some(import_info) = self.lookup_constructor_import(current_module, name) {
                return self.resolve_constructor_in_module(
                    &import_info.source_module,
                    &import_info.original_name,
                );
            }

            // If this name was aliased away, don't fall through to the global table.
            if self.is_name_aliased_away(current_module, name) {
                return Ok(None);
            }

            // Then direct lookup
            Ok(self.lookup_constructor(name))
        } else {
            // Qualified path - could be Type::Constructor or module::Constructor
            let item_name = &path.item_name().name;

            // For two-segment paths like "MyType::A", first check if the first segment
            // is a type name (imported or local)
            if path.segments.len() == 2 {
                let first_segment = &path.segments[0].name;

                // Check if this is an imported type
                if let Some(import_info) = self.lookup_type_import(current_module, first_segment) {
                    // The type is imported - look up constructor in its source module
                    // Constructors from ADTs are registered with the same module as the type
                    return self
                        .resolve_constructor_in_module(&import_info.source_module, item_name);
                }

                // Check if it's a local type (check if there's a constructor with this type name)
                if let Some(ctor_info) = self.lookup_constructor(item_name) {
                    // Verify the constructor's type matches the first segment
                    if ctor_info.type_name == *first_segment {
                        return Ok(Some(ctor_info));
                    }
                }
            }

            // Fall back to module path resolution
            let module_path = ModulePath::new(
                path.module_segments()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
            );

            self.resolve_constructor_in_module(&module_path, item_name)
        }
    }

    /// Resolve a constructor in a specific module.
    fn resolve_constructor_in_module(
        &self,
        module_path: &ModulePath,
        item_name: &str,
    ) -> Result<Option<&ConstructorInfo>, PathResolutionError> {
        // Check if module exists
        if !self.has_module(module_path) {
            return Err(PathResolutionError::ModuleNotFound(module_path.clone()));
        }

        // Check if item is defined in that module using item_modules
        if let Some(item_module) = self.get_item_module(item_name) {
            if item_module == module_path {
                return Ok(self.lookup_constructor(item_name));
            }
        }

        // Fallback: check if constructor is in module's constructor list
        // This is more reliable than item_modules which can have collisions
        if let Some(contents) = self.get_module(module_path) {
            if contents.constructors.iter().any(|c| c == item_name) {
                return Ok(self.lookup_constructor(item_name));
            }
        }

        Ok(None)
    }
}
