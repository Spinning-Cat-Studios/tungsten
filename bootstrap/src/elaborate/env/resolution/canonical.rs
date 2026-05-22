//! Canonical module resolution for re-export chains.
//!
//! Follows import chains to find the module where a type or constructor
//! is originally defined, rather than where it's re-exported from.

use std::collections::HashSet;

use crate::elaborate::env::{Env, ModulePath};

/// Error when resolving canonical module path.
#[derive(Debug, Clone)]
pub enum CanonicalResolutionError {
    /// Cycle detected in re-export chain
    Cycle(Vec<ModulePath>),
    /// Module not found
    ModuleNotFound(ModulePath),
}

impl Env {
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
