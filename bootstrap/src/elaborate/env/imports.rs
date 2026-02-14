//! Import management for module-scoped imports.

use std::collections::HashSet;

use super::{Env, ModulePath};
use crate::span::Span;

/// Information about an imported item, including where it was imported.
///
/// This tracks the origin and location of imports for:
/// 1. Error reporting: show both import locations for duplicates
/// 2. Diagnostics: "imported from both A and B" messages
/// 3. Glob expansion: only re-export items imported with `pub use`
/// 4. Canonical resolution: follow re-export chains to the original definition
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The module from which this item was imported (immediate source)
    pub source_module: ModulePath,
    /// The original name in the source module
    pub original_name: String,
    /// The span where this import was written in the source
    pub import_span: Span,
    /// Whether this import is a re-export (`pub use` vs private `use`).
    /// Only re-exports are visible through glob imports (`use foo::*`).
    pub is_reexport: bool,
    /// The canonical defining module (follows re-export chains).
    /// For `use parser::option::Option` where parser re-exports from core,
    /// source_module is `parser::option` but canonical_module is `core::option`.
    /// None if not yet resolved or if this is the canonical definition.
    pub canonical_module: Option<ModulePath>,
}

impl ImportInfo {
    /// Create a new ImportInfo.
    pub fn new(
        source_module: ModulePath,
        original_name: String,
        import_span: Span,
        is_reexport: bool,
    ) -> Self {
        Self {
            source_module,
            original_name,
            import_span,
            is_reexport,
            canonical_module: None,
        }
    }

    /// Create a new ImportInfo with a known canonical module.
    pub fn with_canonical(
        source_module: ModulePath,
        original_name: String,
        import_span: Span,
        is_reexport: bool,
        canonical_module: ModulePath,
    ) -> Self {
        Self {
            source_module,
            original_name,
            import_span,
            is_reexport,
            canonical_module: Some(canonical_module),
        }
    }
}

/// Error when resolving canonical module path.
#[derive(Debug, Clone)]
pub enum CanonicalResolutionError {
    /// Cycle detected in re-export chain
    Cycle(Vec<ModulePath>),
    /// Module not found
    ModuleNotFound(ModulePath),
}

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Import management (module-scoped)
    // ─────────────────────────────────────────────────────────────────────────

    /// Add an import for a type to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_type_import(
        &mut self,
        current_module: &ModulePath,
        local_name: String,
        source_module: ModulePath,
        original_name: String,
        span: Span,
        is_reexport: bool,
    ) {
        let info = ImportInfo::new(source_module, original_name, span, is_reexport);
        // Also add to legacy global map for backward compatibility
        self.imported_types.insert(local_name.clone(), info.clone());

        // Add to module-specific imports
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_types.insert(local_name, info);
        }
    }

    /// Add an import for a value to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_value_import(
        &mut self,
        current_module: &ModulePath,
        local_name: String,
        source_module: ModulePath,
        original_name: String,
        span: Span,
        is_reexport: bool,
    ) {
        let info = ImportInfo::new(source_module, original_name, span, is_reexport);
        // Also add to legacy global map for backward compatibility
        self.imported_values
            .insert(local_name.clone(), info.clone());

        // Add to module-specific imports
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_values.insert(local_name, info);
        }
    }

    /// Add an import for a constructor to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_constructor_import(
        &mut self,
        current_module: &ModulePath,
        local_name: String,
        source_module: ModulePath,
        original_name: String,
        span: Span,
        is_reexport: bool,
    ) {
        let info = ImportInfo::new(source_module, original_name, span, is_reexport);
        // Also add to legacy global map for backward compatibility
        self.imported_constructors
            .insert(local_name.clone(), info.clone());

        // Add to module-specific imports
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_constructors.insert(local_name, info);
        }
    }

    /// Check if a name is already imported in the given module.
    pub fn is_imported(&self, current_module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.modules.get(current_module) {
            contents.imported_types.contains_key(name)
                || contents.imported_values.contains_key(name)
                || contents.imported_constructors.contains_key(name)
        } else {
            // Fallback to global for backward compatibility
            self.imported_types.contains_key(name)
                || self.imported_values.contains_key(name)
                || self.imported_constructors.contains_key(name)
        }
    }

    /// Check if a type import exists in the given module.
    pub fn has_type_import(&self, current_module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.modules.get(current_module) {
            contents.imported_types.contains_key(name)
        } else {
            self.imported_types.contains_key(name)
        }
    }

    /// Check if a value import exists in the given module.
    pub fn has_value_import(&self, current_module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.modules.get(current_module) {
            contents.imported_values.contains_key(name)
        } else {
            self.imported_values.contains_key(name)
        }
    }

    /// Check if a constructor import exists in the given module.
    pub fn has_constructor_import(&self, current_module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.modules.get(current_module) {
            contents.imported_constructors.contains_key(name)
        } else {
            self.imported_constructors.contains_key(name)
        }
    }

    /// Look up a type import in the given module's scope.
    ///
    /// For multi-file compilation, only looks in module-specific imports.
    /// Global fallback is only used for single-file compilation where modules is empty.
    pub fn lookup_type_import(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<&ImportInfo> {
        if let Some(contents) = self.modules.get(current_module) {
            // Module found - only look in module-specific imports
            return contents.imported_types.get(name);
        }
        // No module entry - single-file mode, use global map
        self.imported_types.get(name)
    }

    /// Look up a value import in the given module's scope.
    ///
    /// For multi-file compilation, only looks in module-specific imports.
    /// Global fallback is only used for single-file compilation where modules is empty.
    pub fn lookup_value_import(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<&ImportInfo> {
        if let Some(contents) = self.modules.get(current_module) {
            // Module found - only look in module-specific imports
            return contents.imported_values.get(name);
        }
        // No module entry - single-file mode, use global map
        self.imported_values.get(name)
    }

    /// Look up a constructor import in the given module's scope.
    ///
    /// For multi-file compilation, only looks in module-specific imports.
    /// Global fallback is only used for single-file compilation where modules is empty.
    pub fn lookup_constructor_import(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<&ImportInfo> {
        if let Some(contents) = self.modules.get(current_module) {
            // Module found - only look in module-specific imports
            return contents.imported_constructors.get(name);
        }
        // No module entry - single-file mode, use global map
        self.imported_constructors.get(name)
    }

    /// Get the source module for a type import, if one exists.
    /// Returns the (source_module, original_name) tuple for backward compatibility.
    pub fn get_type_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_type_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    /// Get the source module for a value import, if one exists.
    /// Returns the (source_module, original_name) tuple for backward compatibility.
    pub fn get_value_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_value_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    /// Get the source module for a constructor import, if one exists.
    /// Returns the (source_module, original_name) tuple for backward compatibility.
    pub fn get_constructor_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_constructor_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Canonical type resolution (ADR 31)
    // ─────────────────────────────────────────────────────────────────────────

    /// Resolve the canonical defining module for a type, following re-export chains.
    ///
    /// Given a type name and a starting module, follows the import chain until
    /// reaching the module where the type is actually defined (not re-exported).
    ///
    /// Uses a visited set to detect cycles in re-export chains.
    ///
    /// Returns:
    /// - `Ok(Some(module))` if canonical module found
    /// - `Ok(None)` if type is defined locally in start_module (no imports needed)
    /// - `Err(Cycle)` if a re-export cycle is detected
    pub fn resolve_canonical_type_module(
        &self,
        type_name: &str,
        start_module: &ModulePath,
    ) -> Result<Option<ModulePath>, CanonicalResolutionError> {
        let mut visited: HashSet<ModulePath> = HashSet::new();
        let mut current_module = start_module.clone();
        let mut current_name = type_name.to_string();

        loop {
            // Cycle detection
            if visited.contains(&current_module) {
                return Err(CanonicalResolutionError::Cycle(
                    visited.into_iter().collect(),
                ));
            }
            visited.insert(current_module.clone());

            // Check if this module has the type as an import
            if let Some(contents) = self.modules.get(&current_module) {
                if let Some(import_info) = contents.imported_types.get(&current_name) {
                    // If canonical_module is already resolved, use it directly
                    if let Some(ref canonical) = import_info.canonical_module {
                        return Ok(Some(canonical.clone()));
                    }
                    // Otherwise, follow the import chain
                    current_module = import_info.source_module.clone();
                    current_name = import_info.original_name.clone();
                    continue;
                }

                // Type is not imported - check if it's defined here
                if contents.types.contains(&current_name) {
                    // Found the canonical definition
                    if &current_module == start_module {
                        return Ok(None); // Defined locally
                    }
                    return Ok(Some(current_module));
                }
            }

            // Check the global types map as fallback (for single-file mode or root types)
            if self.types.contains_key(&current_name) {
                // Check if the type's defining module is tracked
                if let Some(defining_module) = self.item_modules.get(&current_name) {
                    if defining_module == start_module {
                        return Ok(None); // Defined locally
                    }
                    return Ok(Some(defining_module.clone()));
                }
                // Type exists but no module tracking - assume local
                return Ok(None);
            }

            // Type not found anywhere
            return Ok(None);
        }
    }

    /// Resolve the canonical defining module for a constructor, following re-export chains.
    pub fn resolve_canonical_constructor_module(
        &self,
        constructor_name: &str,
        start_module: &ModulePath,
    ) -> Result<Option<ModulePath>, CanonicalResolutionError> {
        let mut visited: HashSet<ModulePath> = HashSet::new();
        let mut current_module = start_module.clone();
        let mut current_name = constructor_name.to_string();

        loop {
            if visited.contains(&current_module) {
                return Err(CanonicalResolutionError::Cycle(
                    visited.into_iter().collect(),
                ));
            }
            visited.insert(current_module.clone());

            if let Some(contents) = self.modules.get(&current_module) {
                if let Some(import_info) = contents.imported_constructors.get(&current_name) {
                    if let Some(ref canonical) = import_info.canonical_module {
                        return Ok(Some(canonical.clone()));
                    }
                    current_module = import_info.source_module.clone();
                    current_name = import_info.original_name.clone();
                    continue;
                }

                if contents.constructors.contains(&current_name) {
                    if &current_module == start_module {
                        return Ok(None);
                    }
                    return Ok(Some(current_module));
                }
            }

            if self.constructors.contains_key(&current_name) {
                if let Some(defining_module) = self.item_modules.get(&current_name) {
                    if defining_module == start_module {
                        return Ok(None);
                    }
                    return Ok(Some(defining_module.clone()));
                }
                return Ok(None);
            }

            return Ok(None);
        }
    }
}
