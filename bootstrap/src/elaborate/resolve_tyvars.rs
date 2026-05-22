//! TyVar resolution for deferred type references (Phase 1d).
//!
//! During Phase 1c, types may reference other types that haven't been elaborated yet
//! (due to AST order). Those references are stored as TyVars. This module resolves
//! those TyVars to their actual type encodings once all types are elaborated.

use std::collections::HashSet;

use tungsten_core::Type;

use super::env::{Constructor, TypeDefKind};
use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Resolve deferred type references (Phase 1d).
    ///
    /// During Phase 1c, types may reference other types that haven't been elaborated yet
    /// (due to AST order). Those references are stored as TyVars. Now that all types
    /// are elaborated, we resolve those TyVars to their actual type encodings.
    pub(super) fn resolve_deferred_type_references(&mut self) {
        // Collect type names to process (can't iterate and mutate at the same time)
        let type_names: Vec<String> = self.env.iter_types().map(|(k, _)| k.clone()).collect();

        for name in type_names {
            // Get the current type definition
            let type_def = match self.env.lookup_type(&name) {
                Some(td) => td.clone(),
                None => continue,
            };

            // Skip types that are still stubs (shouldn't happen after Phase 1c)
            if matches!(type_def.kind, TypeDefKind::Stub) {
                continue;
            }

            // Resolve TyVars in the type's body
            let resolved_kind = match &type_def.kind {
                TypeDefKind::Alias(ty) => {
                    let resolved = self.resolve_tyvars_in_type(ty, &name);
                    TypeDefKind::Alias(resolved)
                }
                TypeDefKind::Record(fields) => {
                    let resolved_fields: Vec<_> = fields
                        .iter()
                        .map(|(field_name, field_ty)| {
                            (
                                field_name.clone(),
                                self.resolve_tyvars_in_type(field_ty, &name),
                            )
                        })
                        .collect();
                    TypeDefKind::Record(resolved_fields)
                }
                TypeDefKind::ADT(ctors) => {
                    let resolved_ctors: Vec<_> = ctors
                        .iter()
                        .map(|ctor| Constructor {
                            name: ctor.name.clone(),
                            fields: ctor
                                .fields
                                .iter()
                                .map(|f| self.resolve_tyvars_in_type(f, &name))
                                .collect(),
                            index: ctor.index,
                            visibility: ctor.visibility,
                            span: ctor.span,
                        })
                        .collect();
                    TypeDefKind::ADT(resolved_ctors)
                }
                TypeDefKind::Stub => continue,
            };

            // Update the type definition with resolved types
            let mut updated = type_def;
            updated.kind = resolved_kind;
            // Clear any cached encoding since we've modified the type
            updated.encoded_type = None;
            self.env.types.insert(name, updated);
        }
    }

    /// Resolve TyVars that refer to defined types.
    ///
    /// This is used in Phase 1d to resolve cross-module type references
    /// that were deferred because the target type wasn't elaborated yet.
    fn resolve_tyvars_in_type(&mut self, ty: &Type, skip_name: &str) -> Type {
        let mut expansion_stack = HashSet::new();
        expansion_stack.insert(skip_name.to_string());
        // Skip mutual recursion group members: their cross-references must remain
        // as TyVars so the encoding pass can convert them to μ-variables
        // (ADR 18.4.26i §5).
        if let Some(group) = self.mutual_recursion_groups.get(skip_name).cloned() {
            for member in &group {
                expansion_stack.insert(member.clone());
            }
        }
        self.resolve_tyvars_in_type_impl(ty, &mut expansion_stack)
    }

    /// Internal implementation of resolve_tyvars_in_type with cycle detection.
    fn resolve_tyvars_in_type_impl(
        &mut self,
        ty: &Type,
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name) if !expansion_stack.contains(Self::strip_named_prefix(name)) => {
                self.resolve_tyvar_definition(name, ty, expansion_stack)
            }
            Type::TyVar(_) => ty.clone(), // In encoding stack or bound variable

            // Binary types: recurse both sides
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                let ra = self.resolve_tyvars_in_type_impl(a, expansion_stack);
                let rb = self.resolve_tyvars_in_type_impl(b, expansion_stack);
                Type::reconstruct_binary(ty, ra, rb)
            }

            // Binding types: recurse into body
            Type::Forall(v, body) | Type::Mu(v, body) => {
                let resolved_body = self.resolve_tyvars_in_type_impl(body, expansion_stack);
                Type::reconstruct_binding(ty, v.clone(), resolved_body)
            }

            Type::Eq(ty_arg, a, b) => Type::eq(
                self.resolve_tyvars_in_type_impl(ty_arg, expansion_stack),
                (**a).clone(),
                (**b).clone(),
            ),

            // Terminal types
            Type::Nat
            | Type::Bool
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => ty.clone(),

            Type::Ptr(inner) | Type::Ref(inner) => {
                let resolved = self.resolve_tyvars_in_type_impl(inner, expansion_stack);
                Type::reconstruct_wrapper(ty, resolved)
            }

            // Deferred type application: resolve and expand
            Type::App(name, args) if !expansion_stack.contains(name) => {
                self.resolve_tyvars_app(name, args, expansion_stack)
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_tyvars_in_type_impl(a, expansion_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }

            // Flat ADT (ADR 2.2.26) - recursively resolve type vars in variants
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|t| self.resolve_tyvars_in_type_impl(t, expansion_stack))
                    .collect(),
                variants
                    .iter()
                    .map(|(ctor, payload)| {
                        (
                            ctor.clone(),
                            self.resolve_tyvars_in_type_impl(payload, expansion_stack),
                        )
                    })
                    .collect(),
            ),
        }
    }

    /// Handle Type::App resolution for resolve_tyvars, extracted to reduce CC.
    fn resolve_tyvars_app(
        &mut self,
        name: &str,
        args: &[Type],
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        // Resolve arguments first
        let resolved_args: Vec<Type> = args
            .iter()
            .map(|a| self.resolve_tyvars_in_type_impl(a, expansion_stack))
            .collect();

        // Add to stack before expanding to detect cycles
        expansion_stack.insert(name.to_string());

        // Try to expand the type application
        let result = if let Some(type_def) = self.env.lookup_type(name).cloned() {
            if !matches!(type_def.kind, TypeDefKind::Stub) {
                match &type_def.kind {
                    TypeDefKind::ADT(_) => {
                        if let Ok(encoded) =
                            self.encode_adt_type_impl(name, &resolved_args, expansion_stack)
                        {
                            encoded
                        } else {
                            Type::app(name.to_string(), resolved_args)
                        }
                    }
                    TypeDefKind::Alias(alias_ty) => {
                        let mut result = alias_ty.clone();
                        for (param, arg) in type_def.params.iter().zip(resolved_args.iter()) {
                            result = result.substitute(param, arg);
                        }
                        self.resolve_tyvars_in_type_impl(&result, expansion_stack)
                    }
                    TypeDefKind::Record(_) => Type::app(name.to_string(), resolved_args),
                    TypeDefKind::Stub => Type::app(name.to_string(), resolved_args),
                }
            } else {
                Type::app(name.to_string(), resolved_args)
            }
        } else {
            Type::app(name.to_string(), resolved_args)
        };

        expansion_stack.remove(name);
        result
    }

    /// Resolve a TyVar that refers to a now-defined type.
    /// Only handles non-parameterized, non-stub types.
    fn resolve_tyvar_definition(
        &mut self,
        name: &str,
        original: &Type,
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        // Strip @-prefix for named types (ADR 13.4.26c §2).
        // TyVars like "@Visibility" reference types stored as "Visibility".
        let lookup_name = Self::strip_named_prefix(name);

        let tracing = self.should_trace_encoding(lookup_name);

        if tracing {
            self.trace_resolve_entry(name, lookup_name);
        }

        let type_def = match self.lookup_resolvable_type(lookup_name, tracing) {
            Some(td) => td,
            None => return original.clone(),
        };

        if tracing {
            let kind = Self::type_def_kind_label(&type_def.kind);
            self.trace_encoding(
                "resolve",
                &format!("  lookup \"{lookup_name}\" → {kind} (0 params)"),
            );
        }

        // No pre-insertion: encode_adt_type_impl manages its own stack entry
        // and handles mutual recursion groups via group pre-insertion (ADR 18.4.26i §5 Step 6).
        let result = match &type_def.kind {
            TypeDefKind::Alias(alias_ty) => {
                self.resolve_tyvars_in_type_impl(alias_ty, expansion_stack)
            }
            TypeDefKind::ADT(_) => self
                .encode_adt_type_impl(lookup_name, &[], expansion_stack)
                .unwrap_or_else(|_| original.clone()),
            TypeDefKind::Record(_) | TypeDefKind::Stub => original.clone(),
        };

        if tracing {
            self.trace_encoding("resolve", &format!("  → {result}"));
        }

        result
    }

    /// Look up a type definition and return it only if it's resolvable
    /// (exists, has no type params, and isn't a stub).
    fn lookup_resolvable_type(
        &mut self,
        lookup_name: &str,
        tracing: bool,
    ) -> Option<super::env::TypeDef> {
        let type_def = match self.env.lookup_type(lookup_name).cloned() {
            Some(td) => td,
            None => {
                if tracing {
                    self.trace_encoding(
                        "resolve",
                        &format!("  lookup \"{lookup_name}\" → not found"),
                    );
                }
                return None;
            }
        };

        if !type_def.params.is_empty() || matches!(type_def.kind, TypeDefKind::Stub) {
            if tracing {
                let reason = if !type_def.params.is_empty() {
                    "has type params"
                } else {
                    "is stub"
                };
                self.trace_encoding("resolve", &format!("  skip: {reason}"));
            }
            return None;
        }

        Some(type_def)
    }

    /// Emit the initial tracing lines for a resolve operation.
    fn trace_resolve_entry(&mut self, name: &str, lookup_name: &str) {
        self.trace_encoding("resolve", &format!("TyVar(\"{name}\")"));
        if name != lookup_name {
            self.trace_encoding("resolve", &format!("  strip \"@\" → \"{lookup_name}\""));
        }
    }

    /// Short label for a TypeDefKind (for tracing output).
    fn type_def_kind_label(kind: &TypeDefKind) -> &'static str {
        match kind {
            TypeDefKind::Alias(_) => "Alias",
            TypeDefKind::ADT(_) => "ADT",
            TypeDefKind::Record(_) => "Record",
            TypeDefKind::Stub => "Stub",
        }
    }

    /// Strip the `@` prefix from named type TyVars.
    /// Named types use `@` prefix to distinguish from genuine type variables
    /// (ADR 13.4.26c §2), but lookups use the bare name.
    fn strip_named_prefix(name: &str) -> &str {
        name.strip_prefix('@').unwrap_or(name)
    }
}
