//! Type analysis — expansion, resolution, recursion detection, and substitution.

use super::strip_named_prefix;
use super::TypeLowering;
use std::collections::HashMap;
use tungsten_core::types::Type;

impl TypeLowering<'_> {
    /// Check whether a `TyVar` name refers to a concrete (registered) ADT or record type
    /// rather than an abstract type variable.
    ///
    /// During elaboration, ADT type names are encoded as `TyVar("TypeName")`
    /// in type positions. Abstract type variables are either bare names like
    /// `TyVar("T")` or `@`-prefixed references like `TyVar("@ModulePath")`.
    /// This method distinguishes the two by checking the cached `concrete_type_names`
    /// set (populated from `adt_types ∪ record_types` during registration).
    ///
    /// The `@`-prefix (a Phase 1c artifact for cross-module references) is
    /// stripped by `strip_named_prefix` before lookup, so both `TyVar("Token")`
    /// and `TyVar("@Token")` resolve to the same concrete type.
    ///
    /// Note: `Type::has_mono_blocking_tyvar` (the shared canonical predicate)
    /// handles `@`-prefix via `!name.starts_with('@')` instead of stripping.
    /// Both approaches produce identical results: `@`-prefixed `TyVars` are never
    /// treated as blocking. This method uses strip-then-lookup because it also
    /// serves non-mono callers that need to resolve `@Name` to a registry entry.
    ///
    /// See ADR 13.4.26c §2 for the full `TyVar` encoding convention.
    #[must_use]
    pub fn is_concrete_named_type(&self, name: &str) -> bool {
        let name = strip_named_prefix(name);
        self.concrete_type_names.contains(name)
    }

    /// Apply current type substitution to a type.
    /// Recursively replaces type variables with their bindings.
    #[must_use]
    pub fn apply_type_subst(&self, ty: &Type) -> Type {
        match ty {
            Type::TyVar(name) => {
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    // Recursively apply in case the substitution contains more type variables
                    self.apply_type_subst(concrete_ty)
                } else {
                    ty.clone()
                }
            }
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(self.apply_type_subst(a)),
                Box::new(self.apply_type_subst(b)),
            ),
            Type::App(name, args) => Type::App(
                name.clone(),
                args.iter().map(|t| self.apply_type_subst(t)).collect(),
            ),
            Type::Forall(v, body) => {
                // Don't substitute under a binding of the same name
                if self.type_subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Forall(v.clone(), Box::new(self.apply_type_subst(body)))
                }
            }
            Type::Mu(v, body) => {
                if self.type_subst.contains_key(v) {
                    ty.clone()
                } else {
                    Type::Mu(v.clone(), Box::new(self.apply_type_subst(body)))
                }
            }
            Type::Eq(_, _, _)
            | Type::Unit
            | Type::Bool
            | Type::Nat
            | Type::String
            | Type::Prop
            | Type::Void => ty.clone(),
            Type::Ptr(inner) => Type::Ptr(Box::new(self.apply_type_subst(inner))),
            Type::Ref(inner) => Type::Ref(Box::new(self.apply_type_subst(inner))),
            // Flat ADT (Phase 2B)
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args.iter().map(|t| self.apply_type_subst(t)).collect(),
                variants
                    .iter()
                    .map(|(vname, vty)| (vname.clone(), self.apply_type_subst(vty)))
                    .collect(),
            ),
            Type::Error => Type::Error,
        }
    }

    /// Expand a type without lowering to LLVM.
    /// Expands `TyVar` (records/ADTs) and App (ADTs) to their structural forms.
    /// Returns None if the type cannot be expanded (not a known record/ADT).
    ///
    /// For n≥3 ADTs, returns `Type::Adt` (not Sum) for consistency with `lower_type`.
    #[must_use]
    pub fn expand_type(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::TyVar(name) => {
                // Strip @-prefix for named types (ADR 13.4.26c §2)
                let name = strip_named_prefix(name);
                // First check if this type variable is bound in current substitution
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    return self.expand_type(concrete_ty);
                }
                // Check if this is a record type
                if let Some(fields) = self.record_types.get(name) {
                    return Some(self.encode_record_type(fields));
                }
                // Check if this is a 0-parameter ADT (sum types written as TyVar)
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        // For n≥3 ADTs, return Type::Adt (not Sum) for consistency
                        if constructors.len() >= 3 {
                            let variants: Vec<(String, Type)> = constructors
                                .iter()
                                .map(|c| {
                                    (
                                        c.name.clone(),
                                        self.encode_constructor_payload(&c.fields, &HashMap::new()),
                                    )
                                })
                                .collect();
                            return Some(Type::Adt(name.to_string(), vec![], variants));
                        }
                        // n≤2: use existing Sum encoding for backwards compat
                        return Some(self.encode_adt_type(constructors, &HashMap::new()));
                    }
                }
                None
            }
            Type::App(name, args) => {
                // Check if this is an ADT type
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    // Build substitution map, applying current type_subst to resolve type variables
                    let subst: HashMap<String, Type> = params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.clone(), self.apply_type_subst(a)))
                        .collect();

                    // For n≥3 ADTs, return Type::Adt for consistency
                    if constructors.len() >= 3 {
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|c| {
                                (
                                    c.name.clone(),
                                    self.encode_constructor_payload(&c.fields, &subst),
                                )
                            })
                            .collect();
                        let resolved_args: Vec<Type> =
                            args.iter().map(|a| self.apply_type_subst(a)).collect();
                        return Some(Type::Adt(name.clone(), resolved_args, variants));
                    }
                    // n≤2: use existing Sum encoding
                    Some(self.encode_adt_type(constructors, &subst))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Resolve a type to its flat ADT representation (`Type::Adt`).
    /// Unlike `expand_type` which returns Sum for compatibility, this returns the
    /// canonical `Type::Adt` form for use with flat ADT codegen.
    ///
    /// For TyVar("Foo") or App("Foo", args), returns `Type::Adt("Foo`", args, variants).
    /// For `Type::Adt`, returns it as-is.
    /// For `Type::Mu` wrapping an Adt, unwraps and returns the inner Adt (non-recursive form).
    /// Returns None if the type is not an ADT.
    #[must_use]
    pub fn resolve_to_flat_adt(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::Adt(_, _, _) => Some(ty.clone()),

            Type::Mu(_, inner) => {
                // For μ X. Adt(...), extract the inner Adt
                self.resolve_to_flat_adt(inner)
            }

            Type::TyVar(name) => {
                // Strip @-prefix for named types (ADR 13.4.26c §2)
                let name = strip_named_prefix(name);
                // First check type substitutions
                if let Some(concrete_ty) = self.type_subst.get(name) {
                    return self.resolve_to_flat_adt(concrete_ty);
                }

                // Check if this is a 0-parameter ADT
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        let variants: Vec<(String, Type)> = constructors
                            .iter()
                            .map(|ctor| {
                                let payload =
                                    self.encode_constructor_payload(&ctor.fields, &HashMap::new());
                                (ctor.name.clone(), payload)
                            })
                            .collect();
                        return Some(Type::Adt(name.to_string(), vec![], variants));
                    }
                }
                None
            }

            Type::App(name, args) => {
                // Check if this is a parameterized ADT
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    let subst: HashMap<String, Type> = params
                        .iter()
                        .zip(args.iter())
                        .map(|(p, a)| (p.clone(), self.apply_type_subst(a)))
                        .collect();

                    let variants: Vec<(String, Type)> = constructors
                        .iter()
                        .map(|ctor| {
                            let payload = self.encode_constructor_payload(&ctor.fields, &subst);
                            (ctor.name.clone(), payload)
                        })
                        .collect();

                    let resolved_args: Vec<Type> =
                        args.iter().map(|a| self.apply_type_subst(a)).collect();

                    return Some(Type::Adt(name.clone(), resolved_args, variants));
                }
                None
            }

            _ => None,
        }
    }

    /// Check if an ADT with the given name is recursive.
    /// An ADT is recursive if any constructor field type contains:
    /// - The Mu variable `α_{name}` (in Mu-encoded form), OR
    /// - The ADT name itself as `TyVar("{name}")` (direct self-reference)
    #[must_use]
    pub fn is_recursive_adt(&self, name: &str) -> bool {
        let mu_var = format!("α_{name}");

        if let Some((_, constructors)) = self.adt_types.get(name) {
            for ctor in constructors {
                for field_ty in &ctor.fields {
                    // Check for both α_{name} and {name} since both can represent recursion
                    if Self::type_mentions_var(field_ty, &mu_var)
                        || Self::type_mentions_var(field_ty, name)
                    {
                        return true;
                    }
                }
            }
            false
        } else {
            false
        }
    }

    /// Check if a type mentions a specific type variable (used for recursion detection).
    pub(super) fn type_mentions_var(ty: &Type, var_name: &str) -> bool {
        match ty {
            Type::TyVar(name) => name == var_name,
            Type::Arrow(t1, t2) | Type::Product(t1, t2) | Type::Sum(t1, t2) => {
                Self::type_mentions_var(t1, var_name) || Self::type_mentions_var(t2, var_name)
            }
            Type::Mu(_, inner) | Type::Forall(_, inner) => Self::type_mentions_var(inner, var_name),
            Type::App(_, type_args) => type_args
                .iter()
                .any(|t| Self::type_mentions_var(t, var_name)),
            Type::Adt(_, _, variants) => variants
                .iter()
                .any(|(_, payload_ty)| Self::type_mentions_var(payload_ty, var_name)),
            Type::Ref(inner) | Type::Ptr(inner) => Self::type_mentions_var(inner, var_name),
            Type::Eq(ty_eq, _, _) => Self::type_mentions_var(ty_eq, var_name),
            Type::Nat | Type::Bool | Type::String | Type::Unit | Type::Prop | Type::Void => false,
            Type::Error => false,
        }
    }

    /// Check if a type is uninhabited (has no values).
    ///
    /// Returns true for:
    /// - `Type::Void` (explicitly uninhabited)
    /// - `Type::App("Never", _)` or `Type::TyVar("Never")` (the Never ADT)
    /// - Any ADT with zero constructors
    ///
    /// This is used to emit LLVM `unreachable` after calls to functions that
    /// return uninhabited types (like `exit` which returns `Never`).
    #[must_use]
    pub fn is_uninhabited_type(&self, ty: &Type) -> bool {
        match ty {
            // Void is explicitly uninhabited
            Type::Void => true,

            // Check for ADT named "Never" (with any number of type args)
            Type::App(name, _) => {
                if name == "Never" {
                    return true;
                }
                // Also check if it's an ADT with zero constructors
                if let Some((_, constructors)) = self.adt_types.get(name) {
                    return constructors.is_empty();
                }
                false
            }

            // Check for 0-parameter ADT named "Never" written as TyVar
            Type::TyVar(name) => {
                // Strip @-prefix for named types (ADR 13.4.26c §2)
                let name = strip_named_prefix(name);
                if name == "Never" {
                    return true;
                }
                // Also check if it's an ADT with zero constructors
                if let Some((params, constructors)) = self.adt_types.get(name) {
                    if params.is_empty() {
                        return constructors.is_empty();
                    }
                }
                false
            }

            // Check flat ADT form
            Type::Adt(name, _, variants) => name == "Never" || variants.is_empty(),

            _ => false,
        }
    }
}
