//! Visibility checking logic for modules and items.

use crate::ast::Visibility;

use super::{ConstructorInfo, Env, ModulePath};

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

    /// Get the visibility of a constructor by looking up its parent type.
    ///
    /// In v1, constructors inherit visibility from their parent type.
    /// Returns None if the parent type is not found.
    pub fn get_constructor_visibility(&self, ctor_info: &ConstructorInfo) -> Option<Visibility> {
        self.lookup_type(&ctor_info.type_name)
            .map(|td| td.visibility)
    }

    /// Check if a constructor is accessible from a given module.
    ///
    /// In v1, constructors inherit visibility from their parent type.
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

        // Get parent type's visibility
        let Some(visibility) = self.get_constructor_visibility(ctor_info) else {
            // If we can't find the parent type, assume accessible (bootstrapping)
            return true;
        };

        self.is_item_accessible(visibility, &ctor_module, from_module, from_same_crate)
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

    /// Get a human-readable string for a visibility level.
    pub fn visibility_name(visibility: Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Crate => "crate-public",
            Visibility::Private => "private",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_module_accessible_root_always_accessible() {
        let env = Env::new();
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");

        // Root module is always accessible
        assert!(env.is_module_accessible(&root, &foo, true));
        assert!(env.is_module_accessible(&root, &root, true));
    }

    #[test]
    fn test_is_module_accessible_public_module() {
        let mut env = Env::new();
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");
        let bar = ModulePath::from_name("bar");

        // Register foo as a public module under root
        env.register_module_with_visibility(foo.clone(), Visibility::Public, Some(root.clone()));

        // Public module is accessible from anywhere in the same crate
        assert!(env.is_module_accessible(&foo, &root, true));
        assert!(env.is_module_accessible(&foo, &bar, true));

        // Not accessible from different crate
        assert!(!env.is_module_accessible(&foo, &bar, false));
    }

    #[test]
    fn test_is_module_accessible_private_module_from_parent() {
        let mut env = Env::new();
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");

        // Register foo as a private module under root
        env.register_module_with_visibility(foo.clone(), Visibility::Private, Some(root.clone()));

        // Private module accessible from parent (root)
        assert!(env.is_module_accessible(&foo, &root, true));
    }

    #[test]
    fn test_is_module_accessible_private_module_from_sibling_parent() {
        let mut env = Env::new();
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");
        let bar = ModulePath::from_name("bar");

        // Register foo as private under root
        env.register_module_with_visibility(foo.clone(), Visibility::Private, Some(root.clone()));
        // Register bar as public under root
        env.register_module_with_visibility(bar.clone(), Visibility::Public, Some(root.clone()));

        // foo is accessible from bar (because bar is also under root, which is foo's parent)
        // In Rust: siblings can access private siblings through the parent
        assert!(env.is_module_accessible(&foo, &bar, true));
    }

    #[test]
    fn test_is_module_accessible_private_nested_module() {
        let mut env = Env::new();
        let root = ModulePath::root();
        let foo = ModulePath::from_name("foo");
        let foo_bar = foo.child("bar");
        let other = ModulePath::from_name("other");

        // Register foo as public under root
        env.register_module_with_visibility(foo.clone(), Visibility::Public, Some(root.clone()));
        // Register foo::bar as private under foo
        env.register_module_with_visibility(
            foo_bar.clone(),
            Visibility::Private,
            Some(foo.clone()),
        );

        // foo::bar is accessible from foo (the parent)
        assert!(env.is_module_accessible(&foo_bar, &foo, true));

        // foo::bar is NOT accessible from other (not a descendant of foo)
        assert!(!env.is_module_accessible(&foo_bar, &other, true));

        // foo::bar is NOT accessible from root (not a descendant of foo, only of foo::bar)
        // Actually, root IS NOT a descendant of foo, so it should be false
        assert!(!env.is_module_accessible(&foo_bar, &root, true));
    }

    #[test]
    fn test_is_module_accessible_unknown_module() {
        let env = Env::new();
        let unknown = ModulePath::from_name("unknown");
        let foo = ModulePath::from_name("foo");

        // Unknown modules are accessible (for bootstrapping)
        assert!(env.is_module_accessible(&unknown, &foo, true));
    }
}
