//! Type encoding cache and pattern building (Encoding Finalization).
//!
//! Caches encoded type representations for reverse lookup from Core types
//! to user-defined names, enabling cleaner error messages.

use tungsten_core::Type;

use super::env::TypeDefKind;
use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Cache encoded types for type name reverse lookup (Encoding Finalization).
    ///
    /// This enables reverse lookup from Core types to user-defined type names
    /// for cleaner error messages.
    ///
    /// - Non-parameterized types (like `Color`) are registered for exact match.
    /// - Parameterized types (like `Option<T>`) are registered as patterns for
    ///   structural matching.
    pub(super) fn cache_type_encodings(&mut self) {
        use crate::driver::{register_type_name, register_type_pattern};

        // Encode in a deterministic, dependency-respecting order
        // (ADR 22.7.26c) — never in `HashMap` iteration order, which made
        // the stored trees' inline depth vary run-to-run.
        let type_names: Vec<String> = self.dependency_respecting_type_order();

        for name in type_names {
            let type_def = match self.env.lookup_type(&name) {
                Some(td) => td.clone(),
                None => continue,
            };

            if type_def.params.is_empty() {
                // Non-parameterized type: register for exact match.
                //
                // Deferred-TyVar Resolution (`resolve_deferred_type_references`) clears
                // `encoded_type` to `None` for every non-stub type, so in the
                // normal flow this loop populates every entry from scratch.
                // The guard keeps the pass idempotent for any entry that
                // arrives already-encoded (a pre-seeded / re-run path): it is
                // left untouched rather than re-encoded. Note the mid-loop
                // `encode_adt_type` cache reads below see a *mix* of encoded
                // (earlier in this loop) and not-yet-encoded referents — which
                // referent is already cached is what made the stored inline
                // depth order-sensitive until `phase1e_encode_order`
                // (ADR 22.7.26c) fixed the iteration order.
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

    /// The deterministic, dependency-respecting type-pass order shared by
    /// Deferred-TyVar Resolution (`resolve_deferred_type_references`, ADR 22.7.26d) and
    /// Encoding Finalization (`cache_type_encodings`, ADR 22.7.26c).
    ///
    /// Nodes are ALL non-stub types — ADTs, records, and aliases,
    /// parameterized ones included (so pattern registration is deterministic
    /// too); edges come from constructor fields, record field types, and
    /// alias bodies (`@`-deferred references included — the edge collector
    /// strips the prefix, so the graph is the same at Phase-1d time as at
    /// Phase-1e time). [`tarjan_scc`] emits SCCs in reverse-topological order
    /// (referents before referrers) and sorts nodes, dependencies, and
    /// within-SCC members lexicographically, so flattening its emission is
    /// both byte-stable across runs and maximally inlining: a non-recursive
    /// referent is always processed before its referrer, so each direct
    /// reference inlines the referent's full resolved (1d) / cached (1e)
    /// form. Genuine recursion cycles still bottom out in μ-bound variables.
    ///
    /// Edges are restricted to **inline-relevant targets** (ADTs and
    /// aliases, ADR 22.7.26d): a reference to a record stays nominal by
    /// design, so a referrer→record edge cannot constrain resolution order —
    /// but left in the graph it welds unrelated types into one giant SCC
    /// through nominal back-edges (main.tg: `Item → TypeDef (record) →
    /// TypeDefBody → TypeExpr → … → Stmt → Item`), and the lexicographic
    /// within-SCC order then resolves a record before its referents, freezing
    /// `@`-deferred references inside its embeds.
    pub(super) fn dependency_respecting_type_order(&self) -> Vec<String> {
        use crate::doctor::audit_mutual_types::encode_order_sccs;
        use std::collections::HashSet;

        let type_refs: Vec<(String, Vec<&Type>)> = self
            .env
            .iter_types()
            .filter(|(_, def)| !matches!(def.kind, TypeDefKind::Stub))
            .map(|(name, def)| {
                let referenced: Vec<&Type> = match &def.kind {
                    TypeDefKind::ADT(ctors) => {
                        ctors.iter().flat_map(|ctor| ctor.fields.iter()).collect()
                    }
                    TypeDefKind::Record(fields) => {
                        fields.iter().map(|(_, field_ty)| field_ty).collect()
                    }
                    TypeDefKind::Alias(ty) => vec![ty],
                    TypeDefKind::Stub => Vec::new(),
                };
                (name.clone(), referenced)
            })
            .collect();

        let inline_relevant_targets: HashSet<String> = self
            .env
            .iter_types()
            .filter(|(_, def)| matches!(def.kind, TypeDefKind::ADT(_) | TypeDefKind::Alias(_)))
            .map(|(name, _)| name.clone())
            .collect();

        // Shared kernel with `info type encode-order` — the diagnostic cannot
        // report an order the compiler does not use (ADR 22.7.26d).
        encode_order_sccs(&type_refs, &inline_relevant_targets)
            .into_iter()
            .flatten()
            .collect()
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
    ///
    /// Only `TyVar` is non-structural (a parameter is replaced by its
    /// argument); every other variant recurses uniformly into its children
    /// via [`Type::map_children`], which also preserves names, binders, and
    /// `Eq` witness terms.
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
            _ => ty.map_children(|child| self.substitute_type_params(child, params, args)),
        }
    }
}
