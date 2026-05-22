//! Type encoding cache and pattern building (Phase 1e).
//!
//! Caches encoded type representations for reverse lookup from Core types
//! to user-defined names, enabling cleaner error messages.

use tungsten_core::Type;

use super::env::TypeDefKind;
use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Cache encoded types for type name reverse lookup (Phase 1e).
    ///
    /// This enables reverse lookup from Core types to user-defined type names
    /// for cleaner error messages.
    ///
    /// - Non-parameterized types (like `Color`) are registered for exact match.
    /// - Parameterized types (like `Option<T>`) are registered as patterns for
    ///   structural matching.
    pub(super) fn cache_type_encodings(&mut self) {
        use crate::driver::{register_type_name, register_type_pattern};

        // Collect all non-stub type names
        let type_names: Vec<String> = self
            .env
            .iter_types()
            .filter(|(_, def)| !matches!(def.kind, TypeDefKind::Stub))
            .map(|(name, _)| name.clone())
            .collect();

        for name in type_names {
            let type_def = match self.env.lookup_type(&name) {
                Some(td) => td.clone(),
                None => continue,
            };

            if type_def.params.is_empty() {
                // Non-parameterized type: register for exact match
                if type_def.encoded_type.is_some() {
                    continue; // Already cached
                }

                let encoded = match &type_def.kind {
                    TypeDefKind::Alias(ty) => Some(ty.clone()),
                    TypeDefKind::Record(fields) => Some(self.encode_record_type(fields)),
                    TypeDefKind::ADT(_) => self.encode_adt_type(&name, &[]).ok(),
                    TypeDefKind::Stub => None,
                };

                if let Some(encoded) = encoded.clone() {
                    if let Some(def) = self.env.types.get_mut(&name) {
                        def.encoded_type = Some(encoded.clone());
                    }
                    register_type_name(encoded, name.clone());
                }
            } else {
                // Parameterized type: register as a pattern
                // Build a pattern with TyVar placeholders for each parameter
                let pattern = self.build_type_pattern(&name, &type_def);
                if let Some(pattern) = pattern {
                    register_type_pattern(pattern);
                }
            }
        }
    }

    /// Build a type pattern for a parameterized type.
    ///
    /// For `Option<T>`, returns a pattern `Unit + TyVar("T")`.
    /// For `List<T>`, returns a pattern `μα_List. Unit + (TyVar("T") × TyVar("α_List"))`.
    fn build_type_pattern(
        &mut self,
        name: &str,
        type_def: &super::env::TypeDef,
    ) -> Option<crate::driver::TypePattern> {
        use crate::driver::TypePattern;

        // Create type args as TyVars for the pattern
        let type_args: Vec<Type> = type_def
            .params
            .iter()
            .map(|p| Type::TyVar(p.clone()))
            .collect();

        // Encode the type with TyVar placeholders
        let pattern = match &type_def.kind {
            TypeDefKind::ADT(_) => self.encode_adt_type(name, &type_args).ok()?,
            TypeDefKind::Record(fields) => {
                // For records with type params, substitute in the pattern
                self.encode_record_type_with_args(fields, &type_def.params, &type_args)
            }
            TypeDefKind::Alias(ty) => {
                // Substitute type params in the alias body
                self.substitute_type_params(ty, &type_def.params, &type_args)
            }
            TypeDefKind::Stub => return None,
        };

        // Check if this is a recursive type (has a μ-binder)
        let mu_var = match &pattern {
            Type::Mu(v, _) => Some(v.clone()),
            _ => None,
        };

        Some(TypePattern {
            name: name.to_string(),
            params: type_def.params.clone(),
            pattern,
            mu_var,
        })
    }

    /// Encode a record type with explicit type arguments substituted.
    fn encode_record_type_with_args(
        &self,
        fields: &[(String, Type)],
        params: &[String],
        args: &[Type],
    ) -> Type {
        // Substitute type params in each field
        let substituted_fields: Vec<(String, Type)> = fields
            .iter()
            .map(|(name, ty)| (name.clone(), self.substitute_type_params(ty, params, args)))
            .collect();
        self.encode_record_type(&substituted_fields)
    }

    /// Substitute type parameters in a type.
    pub(super) fn substitute_type_params(
        &self,
        ty: &Type,
        params: &[String],
        args: &[Type],
    ) -> Type {
        match ty {
            Type::TyVar(v) => {
                // Check if this is a type parameter to substitute
                if let Some(idx) = params.iter().position(|p| p == v) {
                    args.get(idx).cloned().unwrap_or_else(|| ty.clone())
                } else {
                    ty.clone()
                }
            }
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Forall(v, body) => Type::Forall(
                v.clone(),
                Box::new(self.substitute_type_params(body, params, args)),
            ),
            Type::Mu(v, body) => Type::Mu(
                v.clone(),
                Box::new(self.substitute_type_params(body, params, args)),
            ),
            Type::Ptr(inner) => {
                Type::Ptr(Box::new(self.substitute_type_params(inner, params, args)))
            }
            Type::Ref(inner) => {
                Type::Ref(Box::new(self.substitute_type_params(inner, params, args)))
            }
            Type::Eq(inner_ty, t1, t2) => Type::Eq(
                Box::new(self.substitute_type_params(inner_ty, params, args)),
                t1.clone(),
                t2.clone(),
            ),
            Type::App(name, type_args) => Type::App(
                name.clone(),
                type_args
                    .iter()
                    .map(|a| self.substitute_type_params(a, params, args))
                    .collect(),
            ),
            // Base types pass through unchanged
            _ => ty.clone(),
        }
    }
}
