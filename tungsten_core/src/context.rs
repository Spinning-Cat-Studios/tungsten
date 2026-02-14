//! Typing Context
//!
//! A typing context Γ contains:
//! - term variable bindings: x : τ
//! - type variable bindings: α (in scope)

use std::collections::HashSet;
use std::fmt;

use crate::types::{TyVar, Type};

/// A term variable name
pub type Var = String;

/// A binding in the context
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// Term variable binding x : τ
    Term(Var, Type),
    /// Type variable binding α
    TypeVar(TyVar),
}

/// A typing context Γ
///
/// Contains term bindings (x : τ) and type variable bindings (α).
/// Implemented as a list for simplicity (later bindings shadow earlier ones).
#[derive(Debug, Clone, Default)]
pub struct Context {
    bindings: Vec<Binding>,
}

impl Context {
    /// Create an empty context
    #[must_use]
    pub fn new() -> Self {
        Context {
            bindings: Vec::new(),
        }
    }

    /// Extend context with a term binding: Γ, x : τ
    pub fn with_term(&self, var: impl Into<String>, ty: Type) -> Self {
        let mut ctx = self.clone();
        ctx.bindings.push(Binding::Term(var.into(), ty));
        ctx
    }

    /// Extend context with a type variable: Γ, α
    pub fn with_type_var(&self, var: impl Into<String>) -> Self {
        let mut ctx = self.clone();
        ctx.bindings.push(Binding::TypeVar(var.into()));
        ctx
    }

    /// Look up a term variable's type: x : τ ∈ Γ
    #[must_use]
    pub fn lookup_term(&self, var: &str) -> Option<&Type> {
        // Search from the end (most recent binding) to handle shadowing
        for binding in self.bindings.iter().rev() {
            if let Binding::Term(v, ty) = binding {
                if v == var {
                    return Some(ty);
                }
            }
        }
        None
    }

    /// Check if a type variable is in scope: α ∈ Γ
    #[must_use]
    pub fn has_type_var(&self, var: &str) -> bool {
        self.bindings
            .iter()
            .any(|b| matches!(b, Binding::TypeVar(v) if v == var))
    }

    /// Get all type variables in scope
    #[must_use]
    pub fn type_vars(&self) -> HashSet<TyVar> {
        self.bindings
            .iter()
            .filter_map(|b| {
                if let Binding::TypeVar(v) = b {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all term variables in scope with their types
    #[must_use]
    pub fn term_vars(&self) -> Vec<(&str, &Type)> {
        let mut vars = Vec::new();
        let mut seen = HashSet::new();
        // Reverse to get most recent bindings first
        for binding in self.bindings.iter().rev() {
            if let Binding::Term(v, ty) = binding {
                if !seen.contains(v.as_str()) {
                    vars.push((v.as_str(), ty));
                    seen.insert(v.as_str());
                }
            }
        }
        vars.reverse();
        vars
    }

    /// Check if the context is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Get the number of bindings
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.bindings.is_empty() {
            write!(f, "∅")
        } else {
            let parts: Vec<String> = self
                .bindings
                .iter()
                .map(|b| match b {
                    Binding::Term(v, ty) => format!("{v} : {ty}"),
                    Binding::TypeVar(v) => v.clone(),
                })
                .collect();
            write!(f, "{}", parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = Context::new();
        assert!(ctx.is_empty());
        assert_eq!(ctx.lookup_term("x"), None);
        assert!(!ctx.has_type_var("α"));
    }

    #[test]
    fn test_term_binding() {
        let ctx = Context::new().with_term("x", Type::Nat);
        assert_eq!(ctx.lookup_term("x"), Some(&Type::Nat));
        assert_eq!(ctx.lookup_term("y"), None);
    }

    #[test]
    fn test_type_var_binding() {
        let ctx = Context::new().with_type_var("α").with_type_var("β");
        assert!(ctx.has_type_var("α"));
        assert!(ctx.has_type_var("β"));
        assert!(!ctx.has_type_var("γ"));
    }

    #[test]
    fn test_shadowing() {
        let ctx = Context::new()
            .with_term("x", Type::Nat)
            .with_term("x", Type::Bool);
        // Most recent binding wins
        assert_eq!(ctx.lookup_term("x"), Some(&Type::Bool));
    }

    #[test]
    fn test_mixed_bindings() {
        let ctx = Context::new()
            .with_type_var("α")
            .with_term("x", Type::TyVar("α".into()))
            .with_type_var("β");

        assert!(ctx.has_type_var("α"));
        assert!(ctx.has_type_var("β"));
        assert_eq!(ctx.lookup_term("x"), Some(&Type::TyVar("α".into())));
    }

    #[test]
    fn test_display() {
        let ctx = Context::new()
            .with_type_var("α")
            .with_term("x", Type::Nat)
            .with_term("f", Type::arrow(Type::Nat, Type::Bool));

        let s = ctx.to_string();
        assert!(s.contains("α"));
        assert!(s.contains("x : Nat"));
        assert!(s.contains("f : (Nat → Bool)"));
    }
}
