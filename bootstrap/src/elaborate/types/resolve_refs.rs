//! Type reference resolution during ADT encoding.
//!
//! These functions resolve type variable references (`TyVar`, `App`) to their
//! encoded forms during ADT type encoding. They handle cycle detection for
//! mutually recursive types via an `alias_expansion_stack`.
//!
//! The shared `resolve_app_to_encoding` function is the single implementation
//! for resolving `Type::App` nodes to their encoded forms. It is used by
//! both `resolve_type_references_impl` (types/resolve_refs.rs) and
//! `resolve_type_apps_impl` (exprs/helpers.rs). See ADR 20.4.26h §1.

use std::collections::HashSet;

use crate::elaborate::env::{TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use tungsten_core::Type;

/// Selects which recursive traversal to use during alias expansion
/// in `resolve_app_to_encoding`. See ADR 20.4.26h §1.
#[derive(Clone, Copy)]
pub(crate) enum AppResolveMode {
    /// Used by `resolve_type_references_impl` (types/resolve_refs.rs)
    TypeRefs,
    /// Used by `resolve_type_apps_impl` (exprs/helpers.rs)
    TypeApps,
}

impl<'a> Elaborator<'a> {
    /// Resolve type variable references to their encoded forms.
    ///
    /// This handles cases where a field type is `TyVar("RecordName")` -
    /// the record type needs to be expanded to its product encoding.
    ///
    /// Important: This must NOT resolve types that are currently being encoded
    /// (tracked in the alias_expansion_stack to detect cycles).
    #[allow(dead_code)]
    pub(super) fn resolve_type_references(&mut self, ty: &Type, skip_name: &str) -> Type {
        let mut alias_expansion_stack = HashSet::new();
        alias_expansion_stack.insert(skip_name.to_string());
        self.resolve_type_references_impl(ty, &mut alias_expansion_stack)
    }

    /// Internal implementation of type reference resolution with cycle detection.
    pub(crate) fn resolve_type_references_impl(
        &mut self,
        ty: &Type,
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name)
                if !alias_expansion_stack.contains(name)
                    && !alias_expansion_stack.contains(name.strip_prefix('@').unwrap_or(name)) =>
            {
                self.resolve_type_ref_tyvar(name, ty, alias_expansion_stack)
            }
            Type::TyVar(_) => ty.clone(),

            // Binary types: recurse both sides
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                let ra = self.resolve_type_references_impl(a, alias_expansion_stack);
                let rb = self.resolve_type_references_impl(b, alias_expansion_stack);
                Type::reconstruct_binary(ty, ra, rb)
            }

            // Binding types: recurse into body
            Type::Forall(v, body) => Type::forall(
                v,
                self.resolve_type_references_impl(body, alias_expansion_stack),
            ),
            Type::Mu(v, body) => Type::mu(
                v,
                self.resolve_type_references_impl(body, alias_expansion_stack),
            ),

            Type::Eq(ty_arg, a, b) => Type::eq(
                self.resolve_type_references_impl(ty_arg, alias_expansion_stack),
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
                let resolved = self.resolve_type_references_impl(inner, alias_expansion_stack);
                Type::reconstruct_wrapper(ty, resolved)
            }

            // Deferred type application: try to resolve now
            Type::App(name, args) if !alias_expansion_stack.contains(name) => {
                self.resolve_type_refs_app(name, args, alias_expansion_stack)
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_references_impl(a, alias_expansion_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }

            // Flat ADT (ADR 2.2.26): resolve type args and variant payloads
            Type::Adt(name, type_args, variants) => {
                let resolved_args: Vec<Type> = type_args
                    .iter()
                    .map(|a| self.resolve_type_references_impl(a, alias_expansion_stack))
                    .collect();
                let resolved_variants: Vec<(String, Type)> = variants
                    .iter()
                    .map(|(vname, vty)| {
                        (
                            vname.clone(),
                            self.resolve_type_references_impl(vty, alias_expansion_stack),
                        )
                    })
                    .collect();
                Type::adt(name.clone(), resolved_args, resolved_variants)
            }
        }
    }

    /// Resolve a TyVar reference to its encoded form if it's a defined type.
    fn resolve_type_ref_tyvar(
        &mut self,
        name: &str,
        ty: &Type,
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        let tracing = self.should_trace_encoding(name);

        if tracing {
            self.trace_encoding("ref-resolve", &format!("TyVar(\"{name}\")"));
        }

        let type_def = match self.env.lookup_type(name).cloned() {
            Some(td) if td.params.is_empty() => td,
            _ => {
                if tracing {
                    self.trace_encoding("ref-resolve", "  skip (not found or has params)");
                }
                return ty.clone();
            }
        };

        if tracing {
            let kind = match &type_def.kind {
                TypeDefKind::Alias(_) => "Alias",
                TypeDefKind::ADT(_) => "ADT",
                TypeDefKind::Record(_) => "Record",
                TypeDefKind::Stub => "Stub",
            };
            self.trace_encoding("ref-resolve", &format!("  lookup \"{name}\" → {kind}"));
        }

        match &type_def.kind {
            TypeDefKind::Alias(alias_ty) => {
                alias_expansion_stack.insert(name.to_string());
                let result = self.resolve_type_references_impl(alias_ty, alias_expansion_stack);
                alias_expansion_stack.remove(name);
                if tracing {
                    self.trace_encoding("ref-resolve", &format!("  → {result}"));
                }
                result
            }
            TypeDefKind::ADT(_) => {
                // No pre-insertion: encode_adt_type_impl manages its own stack entry
                // and handles mutual recursion groups (ADR 18.4.26i §5 Step 6).
                let result = self
                    .encode_adt_type_impl(name, &[], alias_expansion_stack)
                    .unwrap_or_else(|_| ty.clone());
                if tracing {
                    self.trace_encoding("ref-resolve", &format!("  → {result}"));
                }
                result
            }
            TypeDefKind::Record(_) | TypeDefKind::Stub => ty.clone(),
        }
    }

    /// Handle Type::App resolution for resolve_type_references, extracted to reduce CC.
    fn resolve_type_refs_app(
        &mut self,
        name: &str,
        args: &[Type],
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        // Resolve arguments first
        let resolved_args: Vec<Type> = args
            .iter()
            .map(|a| self.resolve_type_references_impl(a, alias_expansion_stack))
            .collect();

        self.resolve_app_to_encoding(
            name,
            resolved_args,
            alias_expansion_stack,
            AppResolveMode::TypeRefs,
        )
    }

    /// Shared App→encoding resolution used by both `resolve_type_refs_app` and
    /// `resolve_type_apps_app`. See ADR 20.4.26h §1.
    ///
    /// Given a type name and pre-resolved args, attempts to encode the App:
    /// - ADT → delegate to `encode_adt_type_impl` (which manages its own cycle detection)
    /// - Alias → substitute params, recurse via the mode-selected traversal
    /// - Record/Stub → keep as App
    pub(crate) fn resolve_app_to_encoding(
        &mut self,
        name: &str,
        resolved_args: Vec<Type>,
        alias_expansion_stack: &mut HashSet<String>,
        mode: AppResolveMode,
    ) -> Type {
        let Some(type_def) = self.env.lookup_type(name).cloned() else {
            return Type::app(name.to_string(), resolved_args);
        };
        if matches!(type_def.kind, TypeDefKind::Stub) {
            return Type::app(name.to_string(), resolved_args);
        }

        match &type_def.kind {
            TypeDefKind::ADT(_) => {
                // ADTs handle their own cycle detection in encode_adt_type_impl,
                // so we do NOT pre-insert into alias_expansion_stack here.
                self.encode_adt_type_impl(name, &resolved_args, alias_expansion_stack)
                    .unwrap_or_else(|_| Type::app(name.to_string(), resolved_args))
            }
            TypeDefKind::Alias(_) => self.resolve_alias_expansion(
                name,
                &type_def,
                &resolved_args,
                alias_expansion_stack,
                mode,
            ),
            // Intentionally kept as App for Record and Stub:
            TypeDefKind::Record(_) | TypeDefKind::Stub => {
                Type::app(name.to_string(), resolved_args)
            }
        }
    }

    /// Expand a type alias, substituting parameters and resolving the result.
    fn resolve_alias_expansion(
        &mut self,
        name: &str,
        type_def: &TypeDef,
        resolved_args: &[Type],
        alias_expansion_stack: &mut HashSet<String>,
        mode: AppResolveMode,
    ) -> Type {
        let TypeDefKind::Alias(alias_ty) = &type_def.kind else {
            unreachable!("resolve_alias_expansion called on non-alias");
        };
        alias_expansion_stack.insert(name.to_string());
        let mut result = alias_ty.clone();
        for (param, arg) in type_def.params.iter().zip(resolved_args.iter()) {
            result = result.substitute(param, arg);
        }
        let resolved = match mode {
            AppResolveMode::TypeRefs => {
                self.resolve_type_references_impl(&result, alias_expansion_stack)
            }
            AppResolveMode::TypeApps => self.resolve_type_apps_impl(&result, alias_expansion_stack),
        };
        alias_expansion_stack.remove(name);
        resolved
    }

    /// Replace references to the ADT name with the μ type variable.
    pub(super) fn replace_self_reference(&self, ty: &Type, adt_name: &str, mu_var: &str) -> Type {
        match ty {
            // Terminal types: no self-references possible
            Type::Nat
            | Type::Bool
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => ty.clone(),

            Type::TyVar(v) if v == adt_name || v.strip_prefix('@') == Some(adt_name) => {
                Type::TyVar(mu_var.to_string())
            }
            Type::TyVar(_) => ty.clone(),

            // Binary types: recurse both sides
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                let ra = self.replace_self_reference(a, adt_name, mu_var);
                let rb = self.replace_self_reference(b, adt_name, mu_var);
                Type::reconstruct_binary(ty, ra, rb)
            }

            // Binding types: recurse into body
            Type::Forall(v, body) | Type::Mu(v, body) => {
                let resolved = self.replace_self_reference(body, adt_name, mu_var);
                Type::reconstruct_binding(ty, v.clone(), resolved)
            }

            // Eq types: only recurse into the type argument, terms are left alone
            Type::Eq(ty_arg, a, b) => Type::eq(
                self.replace_self_reference(ty_arg, adt_name, mu_var),
                (**a).clone(),
                (**b).clone(),
            ),

            // Pointer/reference types
            Type::Ptr(inner) | Type::Ref(inner) => {
                let resolved = self.replace_self_reference(inner, adt_name, mu_var);
                Type::reconstruct_wrapper(ty, resolved)
            }

            // Deferred type application / Flat ADT: replace if name matches self
            Type::App(name, _args) | Type::Adt(name, _args, _) if name == adt_name => {
                Type::TyVar(mu_var.to_string())
            }
            Type::App(name, args) => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.replace_self_reference(a, adt_name, mu_var))
                    .collect();
                Type::app(name.clone(), resolved_args)
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
}
