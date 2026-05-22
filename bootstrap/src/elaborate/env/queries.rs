//! Query helpers for the environment.
//!
//! "Did you mean" name suggestions and deterministic export for IR cache hashing.

use tungsten_core::Type;

use super::{Env, TypeDef};

impl Env {
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

    // ─────────────────────────────────────────────────────────────────────────
    // Hash export (for IR cache)
    // ─────────────────────────────────────────────────────────────────────────

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
