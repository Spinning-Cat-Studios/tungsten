//! Type encoding utilities — query and comparison helpers.
//!
//! These functions are used by normalization, type comparison, and codegen
//! but are separable from the core ADT/record encoding pipeline in `encoding.rs`.

use crate::elaborate::env::Constructor;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Check if an ADT is recursive (directly or via mutual recursion).
    ///
    /// An ADT is recursive if:
    /// 1. Any constructor field directly references the ADT by name, OR
    /// 2. The ADT is a member of a mutual recursion group (detected by SCC
    ///    in Recursion Grouping). Mutual recursion means the type participates in a
    ///    cycle through other types (e.g., MaybeTypeExpr → TypeExpr → ... →
    ///    MaybeTypeExpr). Such types require fold/unfold for their Mu encoding.
    pub(crate) fn adt_is_recursive(&self, name: &str, constructors: &[Constructor]) -> bool {
        // Check mutual recursion group membership first (fast HashMap lookup)
        let result = if self.mutual_recursion_groups.contains_key(name) {
            true
        } else {
            // Fall back to direct self-reference check
            constructors.iter().any(|ctor| {
                ctor.fields
                    .iter()
                    .any(|field| self.type_references_name(field, name))
            })
        };

        // Debug-mode consistency check (ADR 21.4.26c): every call for a given ADT name
        // must agree on recursiveness. If they disagree, a caller is passing wrong data.
        #[cfg(debug_assertions)]
        {
            let mut map = self.recursiveness_decisions.borrow_mut();
            if let Some(&prev) = map.get(name) {
                debug_assert_eq!(
                    prev, result,
                    "adt_is_recursive disagreement for '{}': was {}, now {}",
                    name, prev, result
                );
            } else {
                map.insert(name.to_string(), result);
            }
        }

        result
    }

    /// Encode a record type as a right-nested product type.
    ///
    /// `{ f1: T1, f2: T2, f3: T3 }` → `T1 × (T2 × T3)`
    ///
    /// Single-field records are encoded as just the field type.
    pub(crate) fn encode_record_type(&self, fields: &[(String, Type)]) -> Type {
        if fields.is_empty() {
            Type::Unit
        } else if fields.len() == 1 {
            fields[0].1.clone()
        } else {
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
    ///
    /// Non-uniform arms: the `TyVar` leaf and the `App`/`Adt` head names
    /// (a name match short-circuits; a miss falls through to the children).
    /// Every other variant delegates to [`Type::children`] (ADR 23.7.26b).
    pub(crate) fn type_references_name(&self, ty: &Type, name: &str) -> bool {
        match ty {
            Type::TyVar(v) => v == name,
            Type::App(head_name, _) if head_name == name => true,
            Type::Adt(adt_name, _, _) if adt_name == name => true,
            _ => ty
                .children()
                .iter()
                .any(|child| self.type_references_name(child, name)),
        }
    }

    /// Check if two types are equal, using normalization and α-equivalence.
    ///
    /// Deliberately NOT on the `children`/`map_children` discipline
    /// (ADR 23.7.26b): the underlying `types_equal_alpha` walks *two* types
    /// in lockstep, not a single tree.
    pub(crate) fn types_equal(&self, a: &Type, b: &Type) -> bool {
        let a_norm = self.normalize_for_comparison(a);
        let b_norm = self.normalize_for_comparison(b);
        tungsten_core::types_equal_alpha(&a_norm, &b_norm)
    }

    /// Check if two terms are definitionally equal at a given type (ADR 21.5.26d).
    ///
    /// Normalizes both sides using `tungsten_core::eval::eval` and compares
    /// structurally. This is scoped to the existing normalizer's capabilities —
    /// no new reduction rules or proof search.
    ///
    /// **This is the whole δ story, and it is "no δ" (ADR 11.8.26b §1.2).**
    /// `tungsten_core::eval::eval` is the *environment-free* evaluator, in which
    /// `Term::Global` is Stuck — so conversion unfolds no constant at all, and a
    /// `#[partial]` one is opaque here for the same reason a certified one is.
    /// 29.6.26e's invariant "the kernel does not unfold a tainted constant during
    /// conversion" therefore already holds, and an `is_delta_reducible` check at
    /// the δ step would guard a step that never happens. Do **not** answer that
    /// by moving the check to `EvalEnv::lookup` instead: that evaluator is
    /// `run`/`test` execution, where partial constants are legitimate — 1043 of
    /// the self-hosted compiler's 2107 definitions are tainted, and refusing to
    /// unfold them would stop the compiler running. If δ-reduction is ever added
    /// here, the admission check belongs beside it.
    pub(crate) fn terms_definitionally_equal(
        &self,
        t1: &tungsten_core::Term,
        t2: &tungsten_core::Term,
        _ty: &Type,
    ) -> bool {
        let n1 = tungsten_core::eval::eval(t1);
        let n2 = tungsten_core::eval::eval(t2);
        n1 == n2
    }

    /// Encode ADT constructors to the canonical comparison shape (for
    /// normalization/pattern matching), without type-parameter substitution.
    ///
    /// Uses the SAME builders as the canonical stored encoder (ADR 21.7.26e
    /// wall 1): left-nested field products via `ctor_fields_product`, and the
    /// ADR 2.2.26 sum policy via `build_adt_sum_body` (2 ctors → `Sum`,
    /// 3+ → `Adt`). This was the third parallel encoder copy — its
    /// right-nested products silently failed `types_pattern_match` for
    /// ≥3-field constructors, and its right-nested `Sum` chains could
    /// false-match a 3+-constructor ADT against a nested sum type argument.
    pub(crate) fn encode_adt_constructors_to_sum(
        &self,
        adt_name: &str,
        params: &[String],
        constructors: &[Constructor],
    ) -> Type {
        let constructor_types: Vec<Type> = constructors
            .iter()
            .map(|ctor| super::encoding::ctor_fields_product(ctor.fields.clone()))
            .collect();
        let param_args: Vec<Type> = params
            .iter()
            .map(|param| Type::TyVar(param.clone()))
            .collect();
        super::encoding::build_adt_sum_body(constructor_types, constructors, adt_name, &param_args)
    }
}

#[cfg(test)]
mod tests {
    use tungsten_core::{Context, Type};

    fn references(ty: &Type, name: &str) -> bool {
        let mut ctx = Context::new();
        let elab = crate::elaborate::Elaborator::new(&mut ctx);
        elab.type_references_name(ty, name)
    }

    #[test]
    fn tyvar_leaf_matches_name() {
        assert!(references(&Type::TyVar("List".into()), "List"));
        assert!(!references(&Type::TyVar("Tree".into()), "List"));
    }

    #[test]
    fn app_head_matches_name() {
        let ty = Type::app("List", vec![Type::Nat]);
        assert!(references(&ty, "List"));
    }

    #[test]
    fn app_arg_matches_via_children() {
        // Head misses, but an arg references the name — found through the
        // children() structural default (ADR 23.7.26b).
        let ty = Type::app("Option", vec![Type::TyVar("List".into())]);
        assert!(references(&ty, "List"));
        assert!(!references(&ty, "Tree"));
    }

    #[test]
    fn adt_name_and_variant_payload_match() {
        let ty = Type::adt("Foo", vec![], vec![("V".into(), Type::TyVar("Bar".into()))]);
        assert!(references(&ty, "Foo"));
        assert!(references(&ty, "Bar"));
        assert!(!references(&ty, "Baz"));
    }

    /// ADR 11.8.26b §1.2: conversion is δ-opaque, so admission has nothing to
    /// gate there.
    ///
    /// Asserted in both polarities on purpose. The `false` case is the invariant
    /// (a global does not reduce to its body); the `true` case is what stops the
    /// test passing vacuously — if `terms_definitionally_equal` answered `false`
    /// for everything, the first assertion alone would still look like proof.
    #[test]
    fn conversion_does_not_unfold_a_global() {
        use tungsten_core::{Context, Term};

        let mut ctx = Context::new();
        let elab = crate::elaborate::Elaborator::new(&mut ctx);
        let global = Term::Global("two".to_string());
        let body = Term::NatLit(2);

        assert!(
            !elab.terms_definitionally_equal(&global, &body, &Type::Nat),
            "a global must stay Stuck through conversion, never δ-reduce to its body"
        );
        assert!(
            elab.terms_definitionally_equal(&global, &global.clone(), &Type::Nat),
            "the same global is still definitionally equal to itself"
        );
        let beta_redex = Term::app(
            Term::lambda("x", Type::Nat, Term::var("x")),
            Term::NatLit(2),
        );
        assert!(
            elab.terms_definitionally_equal(&beta_redex, &Term::NatLit(2), &Type::Nat),
            "reduction that does not need a global still happens — it is δ that is absent, not evaluation"
        );
    }

    #[test]
    fn structural_recursion_through_compound_types() {
        let ty = Type::arrow(
            Type::Nat,
            Type::mu("α_L", Type::product(Type::TyVar("List".into()), Type::Unit)),
        );
        assert!(references(&ty, "List"));
        assert!(!references(&ty, "Tree"));
    }
}
