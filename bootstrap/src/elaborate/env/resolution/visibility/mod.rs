//! Visibility checking logic for modules and items.

#[cfg(test)]
mod tests;

use crate::ast::Visibility;

use crate::elaborate::env::{ConstructorInfo, Env, ModulePath};

impl Env {
    // ─────────────────────────────────────────────────────────────────────────
    // Module visibility
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a module with its visibility.
    ///
    /// `parent` is the module that declared this module (None for root).
    pub fn register_module_with_visibility(
        &mut self,
        path: ModulePath,
        visibility: Visibility,
        parent: Option<ModulePath>,
    ) {
        self.register_module(path.clone());
        self.module_visibility.insert(path, (visibility, parent));
    }

    /// Get the visibility of a module.
    pub fn get_module_visibility(
        &self,
        path: &ModulePath,
    ) -> Option<(Visibility, Option<ModulePath>)> {
        self.module_visibility.get(path).cloned()
    }

    /// Check if a module is accessible from a given location.
    ///
    /// Rust visibility semantics:
    /// - `pub` modules are visible everywhere (within the same crate)
    /// - Private modules are visible only to their parent and descendants of the parent
    ///
    /// Path canonicalization: Both paths are canonicalized before comparison to handle
    /// cases where the same logical module has different path prefixes.
    ///
    /// `from_same_crate` indicates whether we're accessing from the same crate.
    /// Currently always true, but designed for future multi-crate support.
    pub fn is_module_accessible(
        &self,
        target: &ModulePath,
        from: &ModulePath,
        from_same_crate: bool,
    ) -> bool {
        // Root module is always accessible
        if target.is_root() {
            return true;
        }

        // Canonicalize paths for comparison
        let target_canonical = self.canonicalize_path(target);
        let from_canonical = self.canonicalize_path(from);

        // Look up target module's visibility (use original target for lookup)
        let Some((visibility, parent)) = self.get_module_visibility(target) else {
            // If we don't have visibility info, assume accessible (for bootstrapping)
            return true;
        };

        match visibility {
            Visibility::Public => {
                // Public modules are accessible from the same crate
                from_same_crate
            }
            Visibility::Crate => {
                // Crate-public modules are accessible from anywhere in the same crate
                from_same_crate
            }
            Visibility::Private => {
                // Private modules are accessible from:
                // 1. The declaring module (parent)
                // 2. Any descendant of the declaring module
                if let Some(ref declaring_module) = parent {
                    // Canonicalize the declaring module for comparison
                    let declaring_canonical = self.canonicalize_path(declaring_module);
                    // Can access if `from` is the parent or a descendant of parent
                    from_canonical == declaring_canonical
                        || from_canonical.starts_with(&declaring_canonical)
                } else {
                    // Private at root level - only root and its descendants can access
                    // Since there's no parent, only root module items can see it
                    from_canonical.is_root() || from_canonical.starts_with(&ModulePath::root())
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Item visibility checking
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if an item (type, value, or constructor) is accessible from a given module.
    ///
    /// This checks whether the item's visibility allows access from `from_module`.
    /// For constructors, visibility is inherited from the parent type (v1 behavior).
    ///
    /// Path canonicalization: Both `item_module` and `from_module` are canonicalized
    /// before comparison to handle cases where the same logical module has different
    /// path prefixes (e.g., `main::parser::items::types` vs `parser::items::types`).
    ///
    /// # Arguments
    /// * `item_visibility` - The visibility of the item
    /// * `item_module` - The module where the item is defined
    /// * `from_module` - The module attempting to access the item
    /// * `from_same_crate` - Whether access is from the same crate (always true for now)
    pub fn is_item_accessible(
        &self,
        item_visibility: Visibility,
        item_module: &ModulePath,
        from_module: &ModulePath,
        from_same_crate: bool,
    ) -> bool {
        // Canonicalize both paths to handle prefix mismatches
        let item_canonical = self.canonicalize_path(item_module);
        let from_canonical = self.canonicalize_path(from_module);

        match item_visibility {
            Visibility::Public => {
                // Public items are accessible from anywhere (in the same crate for now)
                from_same_crate
            }
            Visibility::Crate => {
                // Crate-public items are accessible from anywhere in the same crate
                from_same_crate
            }
            Visibility::Private => {
                // Private items are accessible from:
                // 1. The defining module itself
                // 2. Any descendant of the defining module
                from_canonical == item_canonical || from_canonical.starts_with(&item_canonical)
            }
        }
    }

    /// Get the effective visibility of a constructor.
    ///
    /// If the constructor has an explicit visibility, use it.
    /// Otherwise, inherit the parent type's visibility.
    /// Returns None if the parent type is not found.
    pub fn get_constructor_visibility(&self, ctor_info: &ConstructorInfo) -> Option<Visibility> {
        let parent_visibility = self
            .lookup_type(&ctor_info.type_name)
            .map(|td| td.visibility)?;
        Some(ctor_info.visibility.unwrap_or(parent_visibility))
    }

    /// Check if a constructor is accessible from a given module.
    ///
    /// Constructors with explicit visibility use that. Otherwise they inherit
    /// the parent type's visibility. A constructor's effective visibility
    /// cannot exceed its parent type's visibility.
    pub fn is_constructor_accessible(
        &self,
        ctor_info: &ConstructorInfo,
        from_module: &ModulePath,
        from_same_crate: bool,
    ) -> bool {
        // Get constructor's module
        let Some(ctor_module) = self.get_item_module(&ctor_info.type_name).cloned() else {
            // If we can't find the module, assume accessible (bootstrapping)
            return true;
        };

        // Get effective constructor visibility
        let Some(visibility) = self.get_constructor_visibility(ctor_info) else {
            // If we can't find the parent type, assume accessible (bootstrapping)
            return true;
        };

        self.is_item_accessible(visibility, &ctor_module, from_module, from_same_crate)
    }

    /// Get the effective visibility of a record field.
    ///
    /// If the field has an explicit visibility (from `field_visibilities`),
    /// use it. Otherwise, inherit the parent type's visibility.
    /// Returns None if the type/field is not found.
    pub fn get_record_field_visibility(
        &self,
        type_name: &str,
        field_index: usize,
    ) -> Option<Visibility> {
        let type_def = self.lookup_type(type_name)?;
        let parent_vis = type_def.visibility;
        let field_vis = type_def
            .field_visibilities
            .get(field_index)
            .copied()
            .flatten();
        Some(field_vis.unwrap_or(parent_vis))
    }

    /// Check if a record field is accessible from a given module.
    pub fn is_record_field_accessible(
        &self,
        type_name: &str,
        field_index: usize,
        from_module: &ModulePath,
        from_same_crate: bool,
    ) -> bool {
        let Some(field_module) = self.get_item_module(type_name).cloned() else {
            return true; // bootstrapping
        };
        let Some(visibility) = self.get_record_field_visibility(type_name, field_index) else {
            return true; // bootstrapping
        };
        self.is_item_accessible(visibility, &field_module, from_module, from_same_crate)
    }

    /// Check if `actual` visibility is at least as visible as `required`.
    ///
    /// Visibility ordering (from most to least visible):
    /// - Public > Crate > Private
    ///
    /// This is used for export validation to ensure that a public item
    /// doesn't reference types with less visibility than itself.
    ///
    /// # Examples
    /// - `visibility_at_least(Public, Public)` → true
    /// - `visibility_at_least(Private, Public)` → false
    /// - `visibility_at_least(Crate, Crate)` → true
    /// - `visibility_at_least(Private, Crate)` → false
    pub fn visibility_at_least(actual: Visibility, required: Visibility) -> bool {
        match required {
            Visibility::Public => actual == Visibility::Public,
            Visibility::Crate => actual == Visibility::Public || actual == Visibility::Crate,
            Visibility::Private => true, // Everything is at least as visible as private
        }
    }

    /// Return the more restrictive of two visibilities (ADR 14.5.26c §2.3).
    ///
    /// Ordering: Public > Crate > Private. `min` picks the narrower scope.
    pub fn min_visibility(a: Visibility, b: Visibility) -> Visibility {
        fn ord(v: Visibility) -> u8 {
            match v {
                Visibility::Private => 0,
                Visibility::Crate => 1,
                Visibility::Public => 2,
            }
        }
        if ord(a) <= ord(b) {
            a
        } else {
            b
        }
    }

    /// Compute the effective visibility of a value accessed through a re-export.
    ///
    /// If `name` was imported into `accessing_module` via a `pub use` with a
    /// `reexport_visibility`, the effective visibility is
    /// `min(declared_visibility, reexport_visibility)` (ADR 14.5.26c §2.3).
    /// Otherwise returns `declared_visibility` unchanged.
    pub fn effective_value_visibility(
        &self,
        name: &str,
        declared_visibility: Visibility,
        accessing_module: &ModulePath,
    ) -> Visibility {
        if let Some(info) = self.lookup_value_import(accessing_module, name) {
            if let Some(reexport_vis) = info.reexport_visibility {
                return Self::min_visibility(declared_visibility, reexport_vis);
            }
        }
        declared_visibility
    }

    /// Compute the effective visibility of a type accessed through a re-export.
    pub fn effective_type_visibility(
        &self,
        name: &str,
        declared_visibility: Visibility,
        accessing_module: &ModulePath,
    ) -> Visibility {
        if let Some(info) = self.lookup_type_import(accessing_module, name) {
            if let Some(reexport_vis) = info.reexport_visibility {
                return Self::min_visibility(declared_visibility, reexport_vis);
            }
        }
        declared_visibility
    }

    /// Compute the effective visibility of a constructor accessed through a re-export.
    pub fn effective_constructor_visibility(
        &self,
        name: &str,
        declared_visibility: Visibility,
        accessing_module: &ModulePath,
    ) -> Visibility {
        if let Some(info) = self.lookup_constructor_import(accessing_module, name) {
            if let Some(reexport_vis) = info.reexport_visibility {
                return Self::min_visibility(declared_visibility, reexport_vis);
            }
        }
        declared_visibility
    }

    /// Get a human-readable string for a visibility level.
    pub fn visibility_name(visibility: Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Crate => "crate-public",
            Visibility::Private => "private",
        }
    }
}
