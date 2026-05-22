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
    ///    in Phase 1c.5). Mutual recursion means the type participates in a
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
    pub(crate) fn type_references_name(&self, ty: &Type, name: &str) -> bool {
        match ty {
            Type::Nat
            | Type::Bool
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => false,
            Type::TyVar(v) => v == name,
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                self.type_references_name(a, name) || self.type_references_name(b, name)
            }
            Type::Forall(_, body) | Type::Mu(_, body) => self.type_references_name(body, name),
            Type::Eq(ty_arg, _, _) => self.type_references_name(ty_arg, name),
            Type::Ptr(inner) | Type::Ref(inner) => self.type_references_name(inner, name),
            Type::App(base_name, args) => {
                base_name == name || args.iter().any(|a| self.type_references_name(a, name))
            }
            Type::Adt(adt_name, type_args, variants) => {
                adt_name == name
                    || type_args.iter().any(|a| self.type_references_name(a, name))
                    || variants
                        .iter()
                        .any(|(_, vty)| self.type_references_name(vty, name))
            }
        }
    }

    /// Check if two types are equal, using normalization and α-equivalence.
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

    /// Encode ADT constructors to a sum type (for normalization/comparison).
    ///
    /// Creates the right-nested sum encoding: A + (B + (C + D))
    /// without type parameter substitution.
    pub(crate) fn encode_adt_constructors_to_sum(&self, constructors: &[Constructor]) -> Type {
        if constructors.is_empty() {
            return Type::Void;
        }
        if constructors.len() == 1 {
            return self.encode_constructor_payload_simple(&constructors[0]);
        }
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
            let mut iter = ctor.fields.iter().rev();
            let mut result = iter.next().unwrap().clone();
            for t in iter {
                result = Type::product(t.clone(), result);
            }
            result
        }
    }
}
