//! Helper utilities for expression elaboration.
//!
//! Contains:
//! - PatternBinding struct for nested pattern matching
//! - Pattern-to-name extraction
//! - Type substitution utilities

use std::collections::{HashMap, HashSet};

use crate::ast::Pattern;
use crate::span::Spanned;
use tungsten_core::Type;

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Binding information collected from a nested pattern.
/// Maps variable names to their types (for env binding).
pub(in crate::elaborate) struct PatternBinding {
    pub var_name: String,
    pub var_ty: Type,
}

impl<'a> Elaborator<'a> {
    /// Extract variable name from a pattern (only variables supported).
    pub(crate) fn pattern_to_name(&self, pattern: &Pattern) -> ElabResult<String> {
        match pattern {
            Pattern::Var(ident) => Ok(ident.name.clone()),
            Pattern::Wildcard(_) => Ok("_".to_string()),
            _ => Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::UnsupportedPattern("complex patterns".to_string()),
            )
            .with_help("use simple variable patterns in Phase 1")),
        }
    }

    /// Substitute type variables in a type using a substitution map.
    pub(super) fn substitute_type_vars(&self, ty: &Type, subst: &HashMap<String, Type>) -> Type {
        match ty {
            Type::TyVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
            Type::Arrow(param, ret) => Type::arrow(
                self.substitute_type_vars(param, subst),
                self.substitute_type_vars(ret, subst),
            ),
            Type::Product(left, right) => Type::product(
                self.substitute_type_vars(left, subst),
                self.substitute_type_vars(right, subst),
            ),
            Type::Sum(left, right) => Type::sum(
                self.substitute_type_vars(left, subst),
                self.substitute_type_vars(right, subst),
            ),
            Type::Forall(param, body) => {
                // Don't substitute bound variables
                let mut new_subst = subst.clone();
                new_subst.remove(param);
                Type::forall(param.clone(), self.substitute_type_vars(body, &new_subst))
            }
            Type::Mu(param, body) => {
                // Don't substitute bound variables
                let mut new_subst = subst.clone();
                new_subst.remove(param);
                Type::mu(param.clone(), self.substitute_type_vars(body, &new_subst))
            }
            Type::App(name, args) => {
                // Substitute in type arguments
                let subst_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.substitute_type_vars(a, subst))
                    .collect();
                Type::app(name.clone(), subst_args)
            }
            _ => ty.clone(),
        }
    }

    /// Substitute recursive type references in a field type.
    /// E.g., for List<T>, substitute the ADT name with the full μ-type.
    ///
    /// When pattern matching on a recursive ADT like `LexErrors = μα_LexErrors. (Unit + (LexError * α_LexErrors))`,
    /// the constructor field types reference the ADT name (e.g., `TyVar("LexErrors")`) for recursive fields.
    /// We need to substitute this with the full μ-type so that recursive fields have the correct type
    /// for function calls expecting `LexErrors` (which is the μ-type).
    pub(super) fn substitute_recursive_refs(&mut self, ty: &Type, adt_type: &Type) -> Type {
        // If adt_type is μα_Foo.F, the stored field types use TyVar("Foo") for recursive references.
        // We need to substitute TyVar("Foo") with the full μ-type.
        if let Type::Mu(mu_var, _) = adt_type {
            // Extract the ADT name from the μ-variable name (e.g., "LexErrors" from "α_LexErrors")
            let adt_name = if let Some(stripped) = mu_var.strip_prefix("α_") {
                stripped
            } else {
                mu_var.as_str()
            };

            // Build substitution: ADT name -> full μ-type
            let mut subst = HashMap::new();
            subst.insert(adt_name.to_string(), adt_type.clone());
            let substituted = self.substitute_type_vars(ty, &subst);

            // After substitution, resolve any Type::App that can now be expanded
            self.resolve_type_apps(&substituted)
        } else if let Type::App(name, args) = adt_type {
            // adt_type is an unresolved App (e.g., App("List", [Token]) from a record field).
            // Resolve it to its μ-encoding, then substitute the recursive self-reference.
            if let Ok(resolved) = self.encode_adt_type(name, args) {
                self.substitute_recursive_refs(ty, &resolved)
            } else {
                self.resolve_type_apps(ty)
            }
        } else {
            // Not a μ-type - still need to resolve any Type::App references
            self.resolve_type_apps(ty)
        }
    }

    /// Resolve Type::App references to their encoded forms.
    ///
    /// This expands deferred type applications like `Type::App("Forest", [Nat])`
    /// to the fully encoded μ-type `μα_Forest. (Unit + ((Nat + (Nat × α_Forest)) × α_Forest))`.
    pub(super) fn resolve_type_apps(&mut self, ty: &Type) -> Type {
        let mut alias_expansion_stack = HashSet::new();
        self.resolve_type_apps_impl(ty, &mut alias_expansion_stack)
    }

    /// Internal implementation of resolve_type_apps with cycle detection.
    pub(crate) fn resolve_type_apps_impl(
        &mut self,
        ty: &Type,
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::App(name, args) if !alias_expansion_stack.contains(name) => {
                self.resolve_type_apps_app(name, args, alias_expansion_stack)
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_apps_impl(a, alias_expansion_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }

            // Binary types: recurse both sides
            Type::Arrow(a, b) => {
                let ra = self.resolve_type_apps_impl(a, alias_expansion_stack);
                let rb = self.resolve_type_apps_impl(b, alias_expansion_stack);
                Type::arrow(ra, rb)
            }
            Type::Product(a, b) => {
                let ra = self.resolve_type_apps_impl(a, alias_expansion_stack);
                let rb = self.resolve_type_apps_impl(b, alias_expansion_stack);
                Type::product(ra, rb)
            }
            Type::Sum(a, b) => {
                let ra = self.resolve_type_apps_impl(a, alias_expansion_stack);
                let rb = self.resolve_type_apps_impl(b, alias_expansion_stack);
                Type::sum(ra, rb)
            }

            // Binding types: recurse into body
            Type::Mu(v, body) => {
                let resolved_body = self.resolve_type_apps_impl(body, alias_expansion_stack);
                Type::mu(v.clone(), resolved_body)
            }
            Type::Forall(v, body) => {
                let resolved_body = self.resolve_type_apps_impl(body, alias_expansion_stack);
                Type::forall(v.clone(), resolved_body)
            }

            // Pointer/reference types
            Type::Ptr(inner) => {
                let resolved = self.resolve_type_apps_impl(inner, alias_expansion_stack);
                Type::ptr(resolved)
            }
            Type::Ref(inner) => {
                let resolved = self.resolve_type_apps_impl(inner, alias_expansion_stack);
                Type::ref_ty(resolved)
            }

            // Leaf types - no App to resolve
            _ => ty.clone(),
        }
    }

    /// Handle Type::App resolution for resolve_type_apps.
    /// Delegates to the shared `resolve_app_to_encoding` (ADR 20.4.26h §1).
    fn resolve_type_apps_app(
        &mut self,
        name: &str,
        args: &[Type],
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        // Resolve arguments first
        let resolved_args: Vec<Type> = args
            .iter()
            .map(|a| self.resolve_type_apps_impl(a, alias_expansion_stack))
            .collect();

        self.resolve_app_to_encoding(
            name,
            resolved_args,
            alias_expansion_stack,
            crate::elaborate::types::resolve_refs::AppResolveMode::TypeApps,
        )
    }
}

#[cfg(test)]
mod tests;
