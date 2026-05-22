//! Import lookup and query methods for Env.

use super::ImportInfo;
use crate::elaborate::env::{Env, ModulePath};

impl Env {
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
            return contents.imported_types.get(name);
        }
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
            return contents.imported_values.get(name);
        }
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
            return contents.imported_constructors.get(name);
        }
        self.imported_constructors.get(name)
    }

    /// Get the source module for a type import, if one exists.
    pub fn get_type_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_type_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    /// Get the source module for a value import, if one exists.
    pub fn get_value_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_value_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    /// Get the source module for a constructor import, if one exists.
    pub fn get_constructor_import_source(
        &self,
        current_module: &ModulePath,
        name: &str,
    ) -> Option<(ModulePath, String)> {
        self.lookup_constructor_import(current_module, name)
            .map(|info| (info.source_module.clone(), info.original_name.clone()))
    }

    /// Check if a name has been aliased away in the given module.
    ///
    /// Returns true if the module has an import where `original_name == name`
    /// but `local_name != name` (i.e., the item was imported under a different name,
    /// so the original name should not be directly accessible).
    pub fn is_name_aliased_away(&self, current_module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.modules.get(current_module) {
            for (local_name, info) in &contents.imported_values {
                if info.original_name == name && local_name != name {
                    return true;
                }
            }
            for (local_name, info) in &contents.imported_types {
                if info.original_name == name && local_name != name {
                    return true;
                }
            }
            for (local_name, info) in &contents.imported_constructors {
                if info.original_name == name && local_name != name {
                    return true;
                }
            }
        }
        false
    }
}
