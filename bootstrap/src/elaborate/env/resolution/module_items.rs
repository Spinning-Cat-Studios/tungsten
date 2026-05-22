//! Module-based item lookup helpers (uses ModuleContents from driver).

use crate::elaborate::env::{ConstructorInfo, Env, ModulePath, TypeDef, ValueDef};

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Module-based item lookup (uses ModuleContents from driver)
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if a type exists in a specific module (uses ModuleContents from driver).
    /// This can be called before collection, as it uses the pre-populated module info.
    pub fn has_type_in_module(&self, module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.get_module(module) {
            contents.types.iter().any(|n| n == name)
        } else {
            false
        }
    }

    /// Check if a value exists in a specific module (uses ModuleContents from driver).
    /// This can be called before collection, as it uses the pre-populated module info.
    pub fn has_value_in_module(&self, module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.get_module(module) {
            contents.values.iter().any(|n| n == name)
        } else {
            false
        }
    }

    /// Check if a constructor exists in a specific module (uses ModuleContents from driver).
    /// This can be called before collection, as it uses the pre-populated module info.
    pub fn has_constructor_in_module(&self, module: &ModulePath, name: &str) -> bool {
        if let Some(contents) = self.get_module(module) {
            contents.constructors.iter().any(|n| n == name)
        } else {
            false
        }
    }

    /// Look up a type in a specific module.
    pub fn lookup_type_in_module(&self, module: &ModulePath, name: &str) -> Option<&TypeDef> {
        // Check if the item belongs to this module
        if let Some(item_module) = self.get_item_module(name) {
            if item_module == module {
                return self.lookup_type(name);
            }
        }
        None
    }

    /// Look up a value in a specific module.
    pub fn lookup_value_in_module(&self, module: &ModulePath, name: &str) -> Option<&ValueDef> {
        // Check if the item belongs to this module
        if let Some(item_module) = self.get_item_module(name) {
            if item_module == module {
                return self.lookup_value(name);
            }
        }
        None
    }

    /// Look up a constructor in a specific module.
    pub fn lookup_constructor_in_module(
        &self,
        module: &ModulePath,
        name: &str,
    ) -> Option<&ConstructorInfo> {
        // Check if the item belongs to this module
        if let Some(item_module) = self.get_item_module(name) {
            if item_module == module {
                return self.lookup_constructor(name);
            }
        }
        None
    }
}
