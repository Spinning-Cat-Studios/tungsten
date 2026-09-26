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
use crate::elaborate::types::ref_walk::TypeRefStrategy;
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
    ///
    /// The traversal itself is the unified walker
    /// [`Elaborator::walk_type_refs`] (`ref_walk.rs`, ADR 23.7.26a) under
    /// [`TypeRefStrategy::Encoding`]: this walker looks up **bare names
    /// only** — a deferred `TyVar("@X")` reaching it embeds unresolved (the
    /// load-bearing asymmetry vs Deferred-TyVar Resolution — ADR 22.7.26d; see the strategy's
    /// doc table).
    pub(crate) fn resolve_type_references_impl(
        &mut self,
        ty: &Type,
        alias_expansion_stack: &mut HashSet<String>,
    ) -> Type {
        self.walk_type_refs(ty, TypeRefStrategy::Encoding, alias_expansion_stack)
    }

    /// Resolve a TyVar reference to its encoded form if it's a defined type.
    ///
    /// ⚠ Resolves the **bare** name only (`lookup_type(name)`) — unlike the
    /// Phase-1d resolver `resolve_tyvar_definition` (in `resolve_tyvars.rs`),
    /// which strips a leading `@` before lookup. `@`-prefixed named references
    /// (ADR 13.4.26c §2) are a Phase-1c/1d-era spelling; by the time this
    /// Phase-1e/encoding path runs they have already been resolved, so a
    /// `TyVar("@Name")` reaching here is treated as an ordinary (unresolvable)
    /// type variable and returned as-is. A caller seeding the env directly for
    /// a Phase-1e test must therefore use bare `TyVar("Name")` references, not
    /// `@`-prefixed ones, to exercise this inline path (ADR 22.7.26c close-out).
    pub(super) fn resolve_type_ref_tyvar(
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

    /// Shared App→encoding resolution used by the unified walker's `Encoding`
    /// strategy (`ref_walk.rs`) and by `resolve_type_apps_app`
    /// (`exprs/helpers/mod.rs`). See ADR 20.4.26h §1.
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
    ///
    /// Method form, kept for the encoder's call sites; the walk itself is
    /// [`replace_self_reference`], a free function (it never needed elaborator
    /// state, and comparator synthesis reuses it from a `ProjectOutput` where
    /// no `Elaborator` exists — ADR 1.8.26c).
    pub(super) fn replace_self_reference(&self, ty: &Type, adt_name: &str, mu_var: &str) -> Type {
        replace_self_reference(ty, adt_name, mu_var)
    }
}

/// Replace references to `adt_name` with `TyVar(mu_var)`.
///
/// A transforming `Type` walker on the [`Type::map_children`] discipline
/// (ADR 23.7.26a follow-up): the only non-uniform arms are the two ways an
/// ADT can name *itself* — a bare (possibly `@`-prefixed) `TyVar`, or a
/// still-deferred `App`/`Adt` head — both rewritten to the μ variable.
/// Every other arm is the structural default, so a future `Type` variant
/// inherits correct recursion here with no edit. The trailing arm is
/// `map_children`, NOT a silent `_ => clone`, so a new variant cannot
/// accidentally stop self-reference replacement inside it.
pub(crate) fn replace_self_reference(ty: &Type, adt_name: &str, mu_var: &str) -> Type {
    match ty {
        // Self-reference as a bare (possibly @-prefixed) type variable.
        Type::TyVar(v) if v == adt_name || v.strip_prefix('@') == Some(adt_name) => {
            Type::TyVar(mu_var.to_string())
        }
        // Self-reference as a (possibly still-deferred) named application
        // or flat ADT head.
        Type::App(name, _args) | Type::Adt(name, _args, _) if name == adt_name => {
            Type::TyVar(mu_var.to_string())
        }
        // Everything else — terminals, non-self TyVars, and every compound
        // (binary/binding/Eq/Ptr/Ref/App/Adt) — is uniform structural
        // recursion, including the `Eq` witness terms which map_children
        // leaves untouched.
        _ => ty.map_children(|child| replace_self_reference(child, adt_name, mu_var)),
    }
}
