//! ADT encoding for normalization.
//!
//! This module handles encoding ADTs as sum-types with μ-binders for recursive types.

use std::collections::{HashMap, HashSet};

use crate::elaborate::env::Constructor;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

use super::NormFieldCtx;

impl<'a> Elaborator<'a> {
    /// Encode an ADT for normalization, properly wrapping recursive types in μ-binders.
    ///
    /// This is used by `normalize_for_comparison_impl` to ensure recursive ADTs are
    /// encoded consistently with how they're inferred from constructors. The key insight
    /// is that recursive ADTs need μ-type wrapping:
    ///
    /// ```text
    /// type List<T> = Nil | Cons(T, List<T>)
    /// // Encoded as: μα_List. Unit + (T × α_List)
    /// ```
    ///
    /// Without this wrapping, comparison of `Pattern` with itself would fail because
    /// the TyVar normalization produces `Sum(...)` while constructor inference produces
    /// `μα_Pattern. Sum(...)`.
    pub(super) fn encode_adt_for_normalization(
        &self,
        name: &str,
        constructors: &[Constructor],
        args: &[Type],
        params: &[String],
        in_progress: &mut HashSet<String>,
    ) -> Type {
        // Check if the ADT is recursive
        let is_recursive = self.adt_is_recursive(name, constructors);

        // Canonicalize type arguments for consistency WITHOUT expanding ADTs.
        // This ensures TyVar("X") and App("X", []) are treated the same,
        // but we don't recursively expand nested ADTs (which would cause asymmetry).
        let normalized_args: Vec<Type> =
            args.iter().map(|a| self.canonicalize_type_arg(a)).collect();

        // Build substitution map for type parameters using normalized args
        let subst: HashMap<&str, &Type> = params
            .iter()
            .zip(normalized_args.iter())
            .map(|(p, a)| (p.as_str(), a))
            .collect();

        // For recursive types, we use a μ-variable like "α_List"
        let mu_var = format!("α_{}", name);

        // Encode each constructor as a product of its fields
        let constructor_types: Vec<Type> = constructors
            .iter()
            .map(|ctor| {
                let mut ctx = NormFieldCtx {
                    adt_name: name,
                    subst: &subst,
                    is_recursive,
                    mu_var: &mu_var,
                    in_progress,
                };
                self.encode_constructor_for_normalization(ctor, &mut ctx)
            })
            .collect();

        // Build sum type from constructors
        let body = self.build_sum_from_constructors(constructor_types);

        // Wrap in μ-type if recursive
        if is_recursive {
            Type::mu(&mu_var, body)
        } else {
            body
        }
    }

    /// Encode a single constructor's payload for normalization.
    pub(super) fn encode_constructor_for_normalization(
        &self,
        ctor: &Constructor,
        ctx: &mut NormFieldCtx,
    ) -> Type {
        if ctor.fields.is_empty() {
            return Type::Unit;
        }

        // Process each field, substituting type parameters and handling recursion
        let field_types: Vec<Type> = ctor
            .fields
            .iter()
            .map(|field_ty| self.normalize_field_for_adt(field_ty, ctx))
            .collect();

        self.build_product_from_fields(field_types)
    }

    /// Build a sum type from constructor encodings.
    ///
    /// Returns Void for empty constructors, the single type for one constructor,
    /// or a right-nested sum (A + (B + C)) for multiple constructors.
    fn build_sum_from_constructors(&self, constructor_types: Vec<Type>) -> Type {
        if constructor_types.is_empty() {
            Type::Void
        } else if constructor_types.len() == 1 {
            constructor_types.into_iter().next().unwrap()
        } else {
            // Multiple constructors: build right-nested sum A + (B + C)
            let mut iter = constructor_types.into_iter().rev();
            let mut sum = iter.next().unwrap();
            for ty in iter {
                sum = Type::sum(ty, sum);
            }
            sum
        }
    }

    /// Build a product type from field types.
    ///
    /// Returns the single type for one field, or a right-nested product
    /// (T1 × (T2 × T3)) for multiple fields.
    fn build_product_from_fields(&self, field_types: Vec<Type>) -> Type {
        if field_types.len() == 1 {
            field_types.into_iter().next().unwrap()
        } else {
            // Multiple fields: build right-nested product T1 × (T2 × T3)
            let mut iter = field_types.into_iter().rev();
            let mut prod = iter.next().unwrap();
            for ty in iter {
                prod = Type::product(ty, prod);
            }
            prod
        }
    }
}
