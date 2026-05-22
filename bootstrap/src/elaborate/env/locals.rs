//! Local scope and type variable management for Env.
//!
//! Manages the local variable binding stack and type variable scope.

use std::collections::HashMap;

use tungsten_core::Type;

use super::definitions::LocalBinding;
use super::Env;

impl Env {
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
    pub(crate) fn lookup_local(&self, name: &str) -> Option<&LocalBinding> {
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
}
