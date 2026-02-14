//! ADT and Record Type Encoding
//!
//! This module handles encoding algebraic data types (ADTs) and records
//! into Core calculus types (sums, products, and μ-types).
//!
//! # Cycle Detection
//!
//! Mutually recursive types (e.g., `type A = ... B ...` and `type B = ... A ...`)
//! would cause infinite recursion during encoding. We prevent this by tracking
//! types currently being encoded and skipping expansion when a cycle is detected.

use std::collections::HashSet;

use crate::elaborate::env::{Constructor, TypeDefKind};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Encode an ADT as a Core type.
    ///
    /// For non-recursive ADTs:
    /// - `type Unit = ()` → `Unit`
    /// - `type Bool = True | False` → `Unit + Unit`
    /// - `type Option<T> = None | Some(T)` → `Unit + T`
    /// - `type Either<A, B> = Left(A) | Right(B)` → `A + B`
    ///
    /// For recursive ADTs (e.g., List<T>):
    /// - `enum List<T> { Nil, Cons(T, List<T>) }` → `μα. Unit + (T × α)`
    pub(crate) fn encode_adt_type(&mut self, name: &str, type_args: &[Type]) -> ElabResult<Type> {
        let mut encoding_stack = HashSet::new();
        self.encode_adt_type_impl(name, type_args, &mut encoding_stack)
    }

    /// Internal implementation of encode_adt_type with cycle detection.
    /// Public within crate so other modules can pass their encoding_stack through.
    pub(crate) fn encode_adt_type_impl(
        &mut self,
        name: &str,
        type_args: &[Type],
        encoding_stack: &mut HashSet<String>,
    ) -> ElabResult<Type> {
        // Cycle detection: if we're already encoding this type, return a reference
        // For 0-arity types, use TyVar for consistency (avoids TyVar vs App("X", []) asymmetry)
        // For parameterized types, use App for type argument tracking
        if encoding_stack.contains(name) {
            return if type_args.is_empty() {
                Ok(Type::TyVar(name.to_string()))
            } else {
                Ok(Type::app(name.to_string(), type_args.to_vec()))
            };
        }

        // Check cache for non-parameterized types (no type_args to substitute)
        if type_args.is_empty() {
            if let Some(type_def) = self.env.lookup_type(name) {
                if let Some(ref cached) = type_def.encoded_type {
                    return Ok(cached.clone());
                }
            }
        }

        let type_def = self.env.lookup_type(name).cloned();
        let Some(type_def) = type_def else {
            return Err(self.undefined_type_error(crate::span::Span::new(0, 0), name));
        };

        let TypeDefKind::ADT(ref constructors) = type_def.kind else {
            return Err(ElabError::new(
                type_def.span,
                ElabErrorKind::Other(format!("`{}` is not an ADT", name)),
            ));
        };

        // Add to encoding stack before processing
        encoding_stack.insert(name.to_string());

        // Check if the ADT is recursive (references itself)
        let is_recursive = self.adt_is_recursive(name, constructors);

        // Build substitution map for type parameters
        let subst: std::collections::HashMap<&str, &Type> = type_def
            .params
            .iter()
            .zip(type_args.iter())
            .map(|(p, a)| (p.as_str(), a))
            .collect();

        // For recursive types, use a fresh type variable for self-reference
        // We use a name like "α_List" to avoid conflicts
        let mu_var = format!("α_{}", name);

        // Encode each constructor as a product of its fields
        let mut constructor_types: Vec<Type> = Vec::new();
        for ctor in constructors {
            let ctor_type = self.encode_constructor_type_impl(
                ctor,
                name,
                &type_def.params,
                &subst,
                is_recursive,
                &mu_var,
                encoding_stack,
            );
            constructor_types.push(ctor_type);
        }

        // Build sum type from constructors
        // Policy (ADR 2.2.26):
        // - n = 0: Void
        // - n = 1: single constructor payload
        // - n = 2: binary sum (existing, compact for Option/Result/Bool)
        // - n >= 3: flat ADT (Type::Adt for O(1) switch dispatch)
        let body = if constructor_types.is_empty() {
            // Empty type (no constructors) = Void
            Type::Void
        } else if constructor_types.len() == 1 {
            // Single constructor: just that type
            constructor_types.into_iter().next().unwrap()
        } else if constructor_types.len() == 2 {
            // Two constructors: binary sum (compact representation)
            // A | B → A + B
            let mut iter = constructor_types.into_iter();
            let left = iter.next().unwrap();
            let right = iter.next().unwrap();
            Type::sum(left, right)
        } else {
            // Three or more constructors: flat ADT (ADR 2.2.26)
            // Build variants list: [(ctor_name, payload_type), ...]
            let variants: Vec<(String, Type)> = constructors
                .iter()
                .zip(constructor_types.into_iter())
                .map(|(ctor, ty)| (ctor.name.clone(), ty))
                .collect();
            Type::adt(name.to_string(), type_args.to_vec(), variants)
        };

        // Remove from encoding stack
        encoding_stack.remove(name);

        // Wrap in μ-type if recursive
        if is_recursive {
            Ok(Type::mu(&mu_var, body))
        } else {
            Ok(body)
        }
    }

    /// Check if an ADT is recursive (any constructor references the ADT itself).
    pub(crate) fn adt_is_recursive(&self, name: &str, constructors: &[Constructor]) -> bool {
        for ctor in constructors {
            for field in &ctor.fields {
                if self.type_references_name(field, name) {
                    return true;
                }
            }
        }
        false
    }

    /// Encode a record type as a right-nested product type.
    ///
    /// `{ f1: T1, f2: T2, f3: T3 }` → `T1 × (T2 × T3)`
    ///
    /// Single-field records are encoded as just the field type.
    pub(crate) fn encode_record_type(&self, fields: &[(String, Type)]) -> Type {
        if fields.is_empty() {
            // Empty record = Unit
            Type::Unit
        } else if fields.len() == 1 {
            // Single-field record = just the field type
            fields[0].1.clone()
        } else {
            // Multiple fields: right-nested product
            let mut iter = fields.iter().rev();
            let (_, last_ty) = iter.next().unwrap();
            let mut product = last_ty.clone();
            for (_, ty) in iter {
                product = Type::product(ty.clone(), product);
            }
            product
        }
    }

    /// Check if a type references a named type.
    fn type_references_name(&self, ty: &Type, name: &str) -> bool {
        match ty {
            Type::TyVar(v) => v == name,
            Type::Arrow(a, b) => {
                self.type_references_name(a, name) || self.type_references_name(b, name)
            }
            Type::Product(a, b) => {
                self.type_references_name(a, name) || self.type_references_name(b, name)
            }
            Type::Sum(a, b) => {
                self.type_references_name(a, name) || self.type_references_name(b, name)
            }
            Type::Forall(_, body) => self.type_references_name(body, name),
            Type::Mu(_, body) => self.type_references_name(body, name),
            // Eq types contain Terms, not Types for the equality arguments
            // These won't appear in ADT constructor fields
            Type::Eq(ty_arg, _, _) => self.type_references_name(ty_arg, name),
            Type::Nat | Type::Bool | Type::Unit | Type::Void | Type::Prop | Type::String => false,
            Type::Ptr(inner) | Type::Ref(inner) => self.type_references_name(inner, name),
            // Deferred type application: check if the base name matches
            Type::App(base_name, args) => {
                base_name == name || args.iter().any(|a| self.type_references_name(a, name))
            }
            // Flat ADT (ADR 2.2.26): check name and type args and variant payloads
            Type::Adt(adt_name, type_args, variants) => {
                adt_name == name
                    || type_args.iter().any(|a| self.type_references_name(a, name))
                    || variants
                        .iter()
                        .any(|(_, vty)| self.type_references_name(vty, name))
            }
        }
    }

    /// Encode a constructor's fields as a product type (with cycle detection).
    fn encode_constructor_type_impl(
        &mut self,
        ctor: &Constructor,
        adt_name: &str,
        adt_params: &[String],
        subst: &std::collections::HashMap<&str, &Type>,
        is_recursive: bool,
        mu_var: &str,
        encoding_stack: &mut HashSet<String>,
    ) -> Type {
        if ctor.fields.is_empty() {
            // Nullary constructor: Unit
            Type::Unit
        } else if ctor.fields.len() == 1 {
            // Single field: just the field type
            self.substitute_in_field_impl(
                &ctor.fields[0],
                adt_name,
                adt_params,
                subst,
                is_recursive,
                mu_var,
                encoding_stack,
            )
        } else {
            // Multiple fields: product type
            let mut fields = ctor.fields.iter();
            let mut product = self.substitute_in_field_impl(
                fields.next().unwrap(),
                adt_name,
                adt_params,
                subst,
                is_recursive,
                mu_var,
                encoding_stack,
            );
            for field in fields {
                let field_ty = self.substitute_in_field_impl(
                    field,
                    adt_name,
                    adt_params,
                    subst,
                    is_recursive,
                    mu_var,
                    encoding_stack,
                );
                product = Type::product(product, field_ty);
            }
            product
        }
    }

    /// Substitute type parameters and self-references in a field type (with cycle detection).
    fn substitute_in_field_impl(
        &mut self,
        field: &Type,
        adt_name: &str,
        _adt_params: &[String],
        subst: &std::collections::HashMap<&str, &Type>,
        is_recursive: bool,
        mu_var: &str,
        encoding_stack: &mut HashSet<String>,
    ) -> Type {
        // First, replace self-references with the μ variable
        let mut result = if is_recursive {
            self.replace_self_reference(field, adt_name, mu_var)
        } else {
            field.clone()
        };

        // Then apply type parameter substitutions
        for (var, replacement) in subst {
            result = result.substitute(var, replacement);
        }

        // Finally, resolve any remaining type references (e.g., record types)
        // that weren't handled by self-reference or substitution
        // Pass the encoding_stack to detect mutual recursion
        result = self.resolve_type_references_impl(&result, encoding_stack);

        result
    }

    /// Resolve type variable references to their encoded forms.
    ///
    /// This handles cases where a field type is `TyVar("RecordName")` -
    /// the record type needs to be expanded to its product encoding.
    ///
    /// Important: This must NOT resolve types that are currently being encoded
    /// (tracked in the encoding_stack to detect cycles).
    #[allow(dead_code)]
    pub(super) fn resolve_type_references(&mut self, ty: &Type, skip_name: &str) -> Type {
        let mut encoding_stack = HashSet::new();
        encoding_stack.insert(skip_name.to_string());
        self.resolve_type_references_impl(ty, &mut encoding_stack)
    }

    /// Internal implementation of type reference resolution with cycle detection.
    fn resolve_type_references_impl(
        &mut self,
        ty: &Type,
        encoding_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name) if !encoding_stack.contains(name) => {
                // Check if this refers to a defined type
                if let Some(type_def) = self.env.lookup_type(name).cloned() {
                    // Only resolve non-parameterized types
                    if type_def.params.is_empty() {
                        match &type_def.kind {
                            TypeDefKind::Alias(alias_ty) => {
                                // Resolve aliases recursively (add to stack to detect cycles)
                                encoding_stack.insert(name.clone());
                                let result =
                                    self.resolve_type_references_impl(alias_ty, encoding_stack);
                                encoding_stack.remove(name);
                                result
                            }
                            TypeDefKind::Record(_) => {
                                // Keep record as nominal type - encoding happens at codegen
                                ty.clone()
                            }
                            TypeDefKind::ADT(_) => {
                                // For ADTs, encode and recursively resolve
                                // Add to stack before encoding to detect mutual recursion
                                encoding_stack.insert(name.clone());
                                let result = if let Ok(encoded) =
                                    self.encode_adt_type_impl(name, &[], encoding_stack)
                                {
                                    encoded
                                } else {
                                    ty.clone()
                                };
                                encoding_stack.remove(name);
                                result
                            }
                            TypeDefKind::Stub => {
                                // Stub - leave as TyVar, will be resolved later
                                ty.clone()
                            }
                        }
                    } else {
                        // Parameterized type used without args - leave as TyVar
                        // (this will be caught as an error elsewhere)
                        ty.clone()
                    }
                } else {
                    // Not a defined type - might be a bound variable
                    ty.clone()
                }
            }
            Type::TyVar(_) => ty.clone(), // In encoding stack or bound variable
            Type::Arrow(a, b) => Type::arrow(
                self.resolve_type_references_impl(a, encoding_stack),
                self.resolve_type_references_impl(b, encoding_stack),
            ),
            Type::Product(a, b) => Type::product(
                self.resolve_type_references_impl(a, encoding_stack),
                self.resolve_type_references_impl(b, encoding_stack),
            ),
            Type::Sum(a, b) => Type::sum(
                self.resolve_type_references_impl(a, encoding_stack),
                self.resolve_type_references_impl(b, encoding_stack),
            ),
            Type::Forall(v, body) => {
                Type::forall(v, self.resolve_type_references_impl(body, encoding_stack))
            }
            Type::Mu(v, body) => {
                Type::mu(v, self.resolve_type_references_impl(body, encoding_stack))
            }
            Type::Eq(ty_arg, a, b) => Type::eq(
                self.resolve_type_references_impl(ty_arg, encoding_stack),
                (**a).clone(),
                (**b).clone(),
            ),
            Type::Nat | Type::Bool | Type::Unit | Type::Void | Type::Prop | Type::String => {
                ty.clone()
            }
            Type::Ptr(inner) => Type::ptr(self.resolve_type_references_impl(inner, encoding_stack)),
            Type::Ref(inner) => {
                Type::ref_ty(self.resolve_type_references_impl(inner, encoding_stack))
            }
            // Deferred type application: try to resolve now
            Type::App(name, args) if !encoding_stack.contains(name) => {
                // Resolve arguments first
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_references_impl(a, encoding_stack))
                    .collect();

                // Check if we can now encode this type
                if let Some(type_def) = self.env.lookup_type(name).cloned() {
                    if !matches!(type_def.kind, TypeDefKind::Stub) {
                        // Type is fully defined, encode it with arguments
                        // Add to stack before encoding to detect mutual recursion
                        encoding_stack.insert(name.clone());
                        let result = match &type_def.kind {
                            TypeDefKind::ADT(_) => {
                                if let Ok(encoded) =
                                    self.encode_adt_type_impl(name, &resolved_args, encoding_stack)
                                {
                                    encoded
                                } else {
                                    Type::app(name.clone(), resolved_args)
                                }
                            }
                            TypeDefKind::Alias(alias_ty) => {
                                // Substitute type params in the alias
                                let mut result = alias_ty.clone();
                                for (param, arg) in type_def.params.iter().zip(resolved_args.iter())
                                {
                                    result = result.substitute(param, arg);
                                }
                                self.resolve_type_references_impl(&result, encoding_stack)
                            }
                            TypeDefKind::Record(_) => {
                                // Keep record as nominal type - encoding happens at codegen
                                // Resolve arguments but keep as App
                                Type::app(name.clone(), resolved_args)
                            }
                            TypeDefKind::Stub => Type::app(name.clone(), resolved_args),
                        };
                        encoding_stack.remove(name);
                        return result;
                    }
                }
                // Couldn't resolve - keep as App with resolved args
                Type::app(name.clone(), resolved_args)
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_references_impl(a, encoding_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }
            // Flat ADT (ADR 2.2.26): resolve type args and variant payloads
            Type::Adt(name, type_args, variants) => {
                let resolved_args: Vec<Type> = type_args
                    .iter()
                    .map(|a| self.resolve_type_references_impl(a, encoding_stack))
                    .collect();
                let resolved_variants: Vec<(String, Type)> = variants
                    .iter()
                    .map(|(vname, vty)| {
                        (
                            vname.clone(),
                            self.resolve_type_references_impl(vty, encoding_stack),
                        )
                    })
                    .collect();
                Type::adt(name.clone(), resolved_args, resolved_variants)
            }
        }
    }

    /// Replace references to the ADT name with the μ type variable.
    fn replace_self_reference(&self, ty: &Type, adt_name: &str, mu_var: &str) -> Type {
        match ty {
            Type::TyVar(v) if v == adt_name => Type::TyVar(mu_var.to_string()),
            Type::TyVar(_) => ty.clone(),
            Type::Arrow(a, b) => Type::arrow(
                self.replace_self_reference(a, adt_name, mu_var),
                self.replace_self_reference(b, adt_name, mu_var),
            ),
            Type::Product(a, b) => Type::product(
                self.replace_self_reference(a, adt_name, mu_var),
                self.replace_self_reference(b, adt_name, mu_var),
            ),
            Type::Sum(a, b) => Type::sum(
                self.replace_self_reference(a, adt_name, mu_var),
                self.replace_self_reference(b, adt_name, mu_var),
            ),
            Type::Forall(v, body) => {
                Type::forall(v, self.replace_self_reference(body, adt_name, mu_var))
            }
            Type::Mu(v, body) => Type::mu(v, self.replace_self_reference(body, adt_name, mu_var)),
            // Eq types: only recurse into the type argument, terms are left alone
            Type::Eq(ty_arg, a, b) => Type::eq(
                self.replace_self_reference(ty_arg, adt_name, mu_var),
                (**a).clone(),
                (**b).clone(),
            ),
            Type::Nat | Type::Bool | Type::Unit | Type::Void | Type::Prop | Type::String => {
                ty.clone()
            }
            Type::Ptr(inner) => Type::ptr(self.replace_self_reference(inner, adt_name, mu_var)),
            Type::Ref(inner) => Type::ref_ty(self.replace_self_reference(inner, adt_name, mu_var)),
            // Deferred type application: replace self-references in arguments
            // Also replace if the base name matches
            Type::App(name, _args) if name == adt_name => Type::TyVar(mu_var.to_string()),
            Type::App(name, args) => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.replace_self_reference(a, adt_name, mu_var))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }
            // Flat ADT (ADR 2.2.26): replace self-references in type args and variants
            Type::Adt(name, _type_args, _variants) if name == adt_name => {
                // The ADT itself references itself - replace with mu_var
                Type::TyVar(mu_var.to_string())
            }
            Type::Adt(name, type_args, variants) => {
                let resolved_args: Vec<Type> = type_args
                    .iter()
                    .map(|a| self.replace_self_reference(a, adt_name, mu_var))
                    .collect();
                let resolved_variants: Vec<(String, Type)> = variants
                    .iter()
                    .map(|(vname, vty)| {
                        (
                            vname.clone(),
                            self.replace_self_reference(vty, adt_name, mu_var),
                        )
                    })
                    .collect();
                Type::adt(name.clone(), resolved_args, resolved_variants)
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 3: Type Equality with Alias Resolution
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if two types are equal, using normalization and α-equivalence.
    ///
    /// This is the main type comparison function for the elaborator. It:
    /// 1. Normalizes both types (expanding record TyVars to Product, ADT Apps to Sum, etc.)
    /// 2. Uses α-equivalence for bound variables in μ-types and ∀-types
    ///
    /// Examples:
    /// - `μα. Unit + α` equals `μβ. Unit + β` (α-equivalence)
    /// - `Point` equals `Nat × Nat` (if Point = { x: Nat, y: Nat })
    pub(crate) fn types_equal(&self, a: &Type, b: &Type) -> bool {
        // Normalize both types to expand record/ADT references
        let a_norm = self.normalize_for_comparison(a);
        let b_norm = self.normalize_for_comparison(b);
        // Use α-equivalent comparison from tungsten_core
        tungsten_core::types_equal_alpha(&a_norm, &b_norm)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Simple Encoding Utilities (for normalization/comparison)
    // ─────────────────────────────────────────────────────────────────────────

    /// Encode ADT constructors to a sum type (for normalization/comparison).
    ///
    /// This creates the right-nested sum encoding: A + (B + (C + D))
    /// without type parameter substitution. For full encoding with substitution,
    /// use `encode_adt_type`.
    ///
    /// Used by `types/normalize.rs` for centralized type normalization.
    pub(crate) fn encode_adt_constructors_to_sum(&self, constructors: &[Constructor]) -> Type {
        if constructors.is_empty() {
            return Type::Void;
        }
        if constructors.len() == 1 {
            return self.encode_constructor_payload_simple(&constructors[0]);
        }
        // Right-nested sum: A + (B + (C + D))
        let mut iter = constructors.iter().rev();
        let mut result = self.encode_constructor_payload_simple(iter.next().unwrap());
        for ctor in iter {
            let payload = self.encode_constructor_payload_simple(ctor);
            result = Type::sum(payload, result);
        }
        result
    }

    /// Encode a single constructor's payload type (simple, no substitution).
    fn encode_constructor_payload_simple(&self, ctor: &Constructor) -> Type {
        if ctor.fields.is_empty() {
            Type::Unit
        } else if ctor.fields.len() == 1 {
            ctor.fields[0].clone()
        } else {
            // Multiple fields: right-nested product
            let mut iter = ctor.fields.iter().rev();
            let mut result = iter.next().unwrap().clone();
            for t in iter {
                result = Type::product(t.clone(), result);
            }
            result
        }
    }
}
