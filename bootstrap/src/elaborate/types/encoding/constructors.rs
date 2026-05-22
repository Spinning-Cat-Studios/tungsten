//! Constructor encoding for ADTs.
//!
//! Extracted from encoding/mod.rs — contains functions that encode
//! individual constructors into product types, substitute field types,
//! build sum bodies, and record μ-provenance.

use std::collections::HashSet;

use super::FieldSubstCtx;
use crate::elaborate::env::Constructor;
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Encode all constructors of an ADT into their product types.
    pub(super) fn encode_constructors(
        &mut self,
        constructors: &[Constructor],
        ctx: &FieldSubstCtx,
        mu_encoding_stack: &mut HashSet<String>,
    ) -> Vec<Type> {
        let mut constructor_types: Vec<Type> = Vec::new();
        for ctor in constructors {
            if ctx.tracing {
                let fields_desc: Vec<String> = ctor.fields.iter().map(|f| format!("{f}")).collect();
                self.trace_encoding(
                    "encode",
                    &format!("  ctor {}: [{}]", ctor.name, fields_desc.join(", ")),
                );
            }
            let ctor_type = self.encode_constructor_type_impl(ctor, ctx, mu_encoding_stack);
            if ctx.tracing {
                self.trace_encoding("encode", &format!("  ctor {} → {ctor_type}", ctor.name));
            }
            constructor_types.push(ctor_type);
        }
        constructor_types
    }

    /// Wrap the encoded body in μ-type if recursive, recording provenance.
    ///
    /// For types in a mutual recursion group, produces nested μ-binders:
    /// `Mu(α_Self, Mu(α_Other1, Mu(α_Other2, body)))`.
    /// Self's binder is outermost; others in lexicographic order.
    pub(super) fn finalize_adt_encoding(
        &mut self,
        type_args: &[Type],
        constructors: &[Constructor],
        body: Type,
        ctx: &FieldSubstCtx,
    ) -> ElabResult<Type> {
        let name = ctx.adt_name;
        if ctx.is_recursive {
            self.record_mu_provenance(&ctx.mu_var, name, type_args, constructors);

            // Build nested μ-binders: innermost first, then wrap outward.
            // Group members (lexicographic order) are inner, self is outermost.
            let mut result = body;
            for (_, member_mu_var) in ctx.group_mu_vars.iter().rev() {
                result = Type::mu(member_mu_var, result);
            }
            result = Type::mu(&ctx.mu_var, result);

            if ctx.tracing {
                self.trace_encoding("encode", &format!("{name}: done → {result}"));
            }
            Ok(result)
        } else {
            if ctx.tracing {
                self.trace_encoding("encode", &format!("{name}: done → {body}"));
            }
            Ok(body)
        }
    }

    /// Build a sum type from encoded constructor payloads.
    ///
    /// Policy (ADR 2.2.26):
    /// - 0 constructors → `Void`
    /// - 1 constructor → bare payload (no Sum wrapper)
    /// - 2 constructors → `Sum(ctor1, ctor2)`
    /// - 3+ constructors → `Adt(name, type_args, [(ctor_name, payload), ...])`
    pub(super) fn build_adt_sum_body(
        constructor_types: Vec<Type>,
        constructors: &[Constructor],
        name: &str,
        type_args: &[Type],
    ) -> Type {
        if constructor_types.is_empty() {
            Type::Void
        } else if constructor_types.len() == 1 {
            constructor_types.into_iter().next().unwrap()
        } else if constructor_types.len() == 2 {
            let mut iter = constructor_types.into_iter();
            let left = iter.next().unwrap();
            let right = iter.next().unwrap();
            Type::sum(left, right)
        } else {
            let variants: Vec<(String, Type)> = constructors
                .iter()
                .zip(constructor_types)
                .map(|(ctor, ty)| (ctor.name.clone(), ty))
                .collect();
            Type::adt(name.to_string(), type_args.to_vec(), variants)
        }
    }

    /// Record provenance for a μ-binder (ADR 13.4.26c §3).
    fn record_mu_provenance(
        &mut self,
        mu_var: &str,
        name: &str,
        type_args: &[Type],
        constructors: &[Constructor],
    ) {
        // Determine whether the new entry has concrete type arguments.
        // Concrete means non-empty and no genuine free TyVars (excluding
        // @-prefixed named type references like @Ident, @Visibility).
        // Pattern/unification calls use TyVar("T") placeholders that would
        // corrupt the provenance needed by the post-elaboration TyVar repair
        // pass (apply_tyvar_substitutions in compile/validation.rs).
        let new_is_concrete = !type_args.is_empty()
            && !type_args
                .iter()
                .any(|ty| ty.free_type_vars().iter().any(|v| !v.starts_with('@')));

        // Only overwrite existing provenance if the new entry is concrete.
        // Non-concrete entries (empty type_args or TyVar placeholders) are
        // recorded only when no existing entry exists.
        if !new_is_concrete && self.type_provenance.mu_origins.contains_key(mu_var) {
            return;
        }

        self.type_provenance.mu_origins.insert(
            mu_var.to_string(),
            crate::elaborate::AdtOrigin {
                adt_name: name.to_string(),
                type_args: type_args.to_vec(),
                constructors: constructors.iter().map(|c| c.name.clone()).collect(),
            },
        );
    }

    /// Encode a constructor's fields as a product type (with cycle detection).
    fn encode_constructor_type_impl(
        &mut self,
        ctor: &Constructor,
        ctx: &FieldSubstCtx,
        mu_encoding_stack: &mut HashSet<String>,
    ) -> Type {
        if ctor.fields.is_empty() {
            Type::Unit
        } else if ctor.fields.len() == 1 {
            self.substitute_in_field_impl(&ctor.fields[0], ctx, mu_encoding_stack)
        } else {
            let mut fields = ctor.fields.iter();
            let mut product =
                self.substitute_in_field_impl(fields.next().unwrap(), ctx, mu_encoding_stack);
            for field in fields {
                let field_ty = self.substitute_in_field_impl(field, ctx, mu_encoding_stack);
                product = Type::product(product, field_ty);
            }
            product
        }
    }

    /// Substitute type parameters and self-references in a field type (with cycle detection).
    fn substitute_in_field_impl(
        &mut self,
        field: &Type,
        ctx: &FieldSubstCtx,
        mu_encoding_stack: &mut HashSet<String>,
    ) -> Type {
        let mut result = if ctx.is_recursive {
            self.replace_self_reference(field, ctx.adt_name, &ctx.mu_var)
        } else {
            field.clone()
        };

        for (member_name, member_mu_var) in &ctx.group_mu_vars {
            result = self.replace_self_reference(&result, member_name, member_mu_var);
        }

        for (var, replacement) in ctx.subst {
            result = result.substitute(var, replacement);
        }

        result = self.resolve_type_references_impl(&result, mu_encoding_stack);

        result
    }
}
