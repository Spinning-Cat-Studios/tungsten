//! ADT encoding for normalization.
//!
//! This module handles encoding ADTs as sum-types with μ-binders for recursive types.

use std::collections::{HashMap, HashSet};

use crate::elaborate::env::Constructor;
use crate::elaborate::types::encoding::ctor_fields_product;
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

        // Build the sum body via the SAME policy helper as the canonical
        // encoder (ADR 2.2.26: 2 ctors → Sum, 3+ → Adt). A parallel
        // right-nested Sum-chain builder here diverged from stored encodings
        // for every 3+-ctor generic ADT (ADR 21.7.26e wall 1).
        let body = crate::elaborate::types::encoding::build_adt_sum_body(
            constructor_types,
            constructors,
            name,
            &normalized_args,
        );

        // Wrap in μ-type if recursive
        if is_recursive {
            Type::mu(&mu_var, body)
        } else {
            body
        }
    }

    /// Encode a single constructor's payload for normalization.
    ///
    /// Field products use the canonical left-nested builder shared with the
    /// stored-encoding path — a right-nested copy here made every ≥3-field
    /// constructor of a generic ADT fail `types_equal` against its own
    /// stored encoding (ADR 21.7.26e wall 1).
    pub(super) fn encode_constructor_for_normalization(
        &self,
        ctor: &Constructor,
        ctx: &mut NormFieldCtx,
    ) -> Type {
        // Process each field, substituting type parameters and handling recursion
        let field_types: Vec<Type> = ctor
            .fields
            .iter()
            .map(|field_ty| self.normalize_field_for_adt(field_ty, ctx))
            .collect();

        ctor_fields_product(field_types)
    }
}
