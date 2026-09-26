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

/// Encode a constructor's field types as the canonical **right**-nested
/// product (`(T1 × (T2 × T3))`) — the layout stored encodings and codegen
/// consume.
///
/// Shared by the canonical encoder and `normalize_for_comparison`'s ADT
/// expansion so the two spellings of the same ADT can never diverge on
/// product associativity again (ADR 21.7.26e wall 1: the normalize-side
/// copy diverged, so every ≥3-field constructor of a generic ADT failed
/// `types_equal` against its own stored encoding).
///
/// # Why right, and why it changed (ADR 1.8.26b D1)
///
/// Until 1.8.26b this built *left*-nested, and was the only left-nested
/// product in the language. Everything a value actually flows through is
/// right-nested, so the type and the value disagreed at arity ≥ 3 — below
/// three there is no nesting and the two coincide, which is why the defect
/// class presents with an exact arity boundary rather than as a general
/// breakage. Four independent witnesses, all measured, all agreeing that the
/// *type* side was the outlier:
///
/// | Witness | Nesting |
/// |---|---|
/// | `build_product_value` (what `AdtConstruct` carries) | right |
/// | tuple types (`parser::types`), values and projections | right |
/// | `wrapping.rs`, the nested-pattern destructurer | right |
/// | the **self-hosted** encoder (`elab/items/collect/types/helpers.tg`) | right |
///
/// The disagreement was not confined to the comparator: `match Box2 { B3(a,
/// b, c) => … }` on a 3-field constructor projected `Fst(Fst(v))` into a
/// right-nested value and went silently Stuck.
pub(crate) fn ctor_fields_product(field_types: Vec<Type>) -> Type {
    let mut fields = field_types.into_iter().rev();
    let Some(last) = fields.next() else {
        return Type::Unit;
    };
    fields.fold(last, |product, field_ty| Type::product(field_ty, product))
}

/// Build a sum type from encoded constructor payloads.
///
/// Policy (ADR 2.2.26):
/// - 0 constructors → `Void`
/// - 1 constructor → bare payload (no Sum wrapper)
/// - 2 constructors → `Sum(ctor1, ctor2)`
/// - 3+ constructors → `Adt(name, type_args, [(ctor_name, payload), ...])`
///
/// A **free function with no elaborator state**, because three producers now
/// share it and only one of them has an `Elaborator`: the canonical stored
/// encoder, `normalize_for_comparison`'s ADT expansion (ADR 21.7.26e wall 1 —
/// its private copy built right-nested `Sum` chains for 3+ constructors,
/// diverging from this policy), and comparator synthesis' instantiation
/// expander (ADR 1.8.26c), which runs from a `ProjectOutput` at gate time and
/// so cannot reach the elaborator at all.
pub(crate) fn build_adt_sum_body(
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
        let field_types: Vec<Type> = ctor
            .fields
            .iter()
            .map(|field| self.substitute_in_field_impl(field, ctx, mu_encoding_stack))
            .collect();
        ctor_fields_product(field_types)
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
