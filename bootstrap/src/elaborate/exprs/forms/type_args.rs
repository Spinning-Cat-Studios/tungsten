//! Type argument extraction, instantiation, and unification helpers.
//!
//! Contains:
//! - Type argument extraction from ADT encodings
//! - Constructor field instantiation (two-phase substitution)
//! - Type unification for building substitutions

use std::collections::HashMap;

use tungsten_core::Type;

use crate::elaborate::{ElabResult, Elaborator};

/// Traversal context for type argument extraction.
struct TypeArgTraversal<'a> {
    type_params: &'a [String],
    subst: &'a mut HashMap<String, Type>,
    depth: usize,
    check_compound_types: bool,
}

impl<'a> Elaborator<'a> {
    /// Build a substitution map from type parameters to concrete types using unification.
    ///
    /// Given an instantiated ADT type (e.g., `Option<Point>` encoded as a sum/μ type),
    /// the type parameters (e.g., `["T"]`), and the ADT name (e.g., `"Option"`),
    /// extract the concrete type arguments by unifying a pattern type with the concrete type.
    ///
    /// This unification approach correctly handles tuple type arguments like `(Nat, Nat)`
    /// which would be incorrectly decomposed by the traversal-based approach.
    ///
    /// This is used when pattern matching on generic ADTs to substitute
    /// type parameters in constructor field types.
    pub(in crate::elaborate::exprs) fn build_type_param_substitution_by_unification(
        &mut self,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> HashMap<String, Type> {
        let mut subst = HashMap::new();

        if type_params.is_empty() {
            return subst;
        }

        // Fast path: if adt_type is App(same_name, args), extract directly
        if let Type::App(name, args) = adt_type {
            if name == adt_name && args.len() == type_params.len() {
                for (param, arg) in type_params.iter().zip(args.iter()) {
                    subst.insert(param.clone(), arg.clone());
                }
                return subst;
            }
        }

        // Create TyVars for each type parameter to build the pattern type
        let type_param_vars: Vec<Type> =
            type_params.iter().map(|p| Type::TyVar(p.clone())).collect();

        // Encode the ADT with type parameter TyVars as arguments
        // This gives us a pattern like: μα_List. (Unit + (TyVar("T") × α_List))
        if let Ok(pattern) = self.encode_adt_type(adt_name, &type_param_vars) {
            // Unify the pattern with the concrete expected type to extract bindings
            self.unify_to_subst(&pattern, adt_type, &mut subst);
        }

        subst
    }

    /// Instantiate constructor field types for a generic ADT.
    ///
    /// This performs the **two-phase substitution** required for generic recursive ADTs:
    ///
    /// 1. **Phase 1: Type parameter substitution** - Substitute generic type parameters
    ///    (e.g., `T → Token` for `Option<Token>`)
    /// 2. **Phase 2: μ-type substitution** - Substitute recursive references with the
    ///    full μ-type (e.g., `List → μα_List. ...` for recursive fields)
    ///
    /// ## Ordering Invariant
    ///
    /// The two phases **must** be performed in this order. Reversing them breaks generic
    /// recursive ADTs because:
    /// - Phase 1 resolves type variables like `T` to concrete types
    /// - Phase 2 then resolves recursive references that may contain the now-concrete types
    ///
    /// See ADR 24.1.26 (Bug #6: Generic Pattern Type Parameter Substitution) for the
    /// detailed analysis that established this ordering requirement.
    ///
    /// ## Example
    ///
    /// For `Result<Token, LexError>` with constructor `Ok(value: T)`:
    /// - Phase 1: `T → Token` gives field type `Token`
    /// - Phase 2: No recursive refs, so `Token` unchanged
    ///
    /// For `List<A>` with constructor `Cons(head: A, tail: List<A>)`:
    /// - Phase 1: `A → Nat` gives field types `Nat`, `List<Nat>`
    /// - Phase 2: `List<Nat> → μα_List. (Unit + (Nat × α_List))` (fully expanded)
    pub(in crate::elaborate::exprs) fn instantiate_constructor_fields(
        &mut self,
        fields: &[Type],
        type_params: &[String],
        adt_type: &Type,
    ) -> Vec<Type> {
        // Extract the ADT name from the μ-type, or empty for non-recursive types
        let adt_name = if let Type::Mu(mu_var, _) = adt_type {
            // Extract name from μ-variable (e.g., "List" from "α_List")
            if let Some(stripped) = mu_var.strip_prefix("α_") {
                stripped
            } else {
                mu_var.as_str()
            }
        } else {
            "" // Non-μ types - need ADT name from caller
        };

        self.instantiate_constructor_fields_with_name(fields, type_params, adt_type, adt_name)
    }

    /// Instantiate constructor field types with explicit ADT name.
    ///
    /// This variant is used when the ADT type is not a μ-type (non-recursive ADTs)
    /// and we need to provide the ADT name explicitly for unification.
    pub(in crate::elaborate::exprs) fn instantiate_constructor_fields_with_name(
        &mut self,
        fields: &[Type],
        type_params: &[String],
        adt_type: &Type,
        adt_name: &str,
    ) -> Vec<Type> {
        // Phase 1: Build type parameter substitution using unification
        // This correctly handles tuple type arguments like (Nat, Nat)
        let type_arg_subst =
            self.build_type_param_substitution_by_unification(adt_type, type_params, adt_name);

        // Phase 2: Apply both substitutions to each field
        fields
            .iter()
            .map(|field_ty| {
                // First substitute type parameters
                let with_type_args = self.substitute_type_vars(field_ty, &type_arg_subst);
                // Then substitute recursive μ-type references
                self.substitute_recursive_refs(&with_type_args, adt_type)
            })
            .collect()
    }

    /// Extract type arguments by looking for leaf types in the structure.
    /// This is a heuristic that works for common ADT encodings.
    ///
    /// The `depth` parameter tracks how deep we are in the structure:
    /// - depth 0: we're at the outer ADT level (don't check if Sum is an ADT)
    /// - depth > 0: we're inside, so Sum types might be type arguments
    ///
    /// The `check_compound_types` flag controls whether to check if Sum/Product
    /// types are known ADT/record encodings. Set to true for pattern matching
    /// (where we need to detect nested ADTs), false for simple extraction.
    ///
    /// This is the canonical implementation - constructors.rs delegates here.
    pub(in crate::elaborate::exprs) fn extract_type_args_into_subst(
        &self,
        ty: &Type,
        type_params: &[String],
        subst: &mut HashMap<String, Type>,
        depth: usize,
        check_compound_types: bool,
    ) {
        let mut ctx = TypeArgTraversal {
            type_params,
            subst,
            depth,
            check_compound_types,
        };
        self.extract_type_args_walk(ty, &mut ctx);
    }

    /// Internal recursive walk for type argument extraction.
    fn extract_type_args_walk(&self, ty: &Type, ctx: &mut TypeArgTraversal<'_>) {
        match ty {
            Type::Mu(_bound_var, body) => {
                ctx.depth += 1;
                self.extract_type_args_walk(body, ctx);
                ctx.depth -= 1;
            }
            Type::Sum(left, right) => {
                if self.try_bind_compound_sum(ty, ctx) {
                    return;
                }
                self.extract_binary_type_args(left, right, ctx);
            }
            Type::Product(left, right) => {
                if self.try_bind_compound_product(ty, ctx) {
                    return;
                }
                self.extract_binary_type_args(left, right, ctx);
            }
            Type::Arrow(param, ret) => {
                self.extract_binary_type_args(param, ret, ctx);
            }
            Type::Nat | Type::Bool | Type::String => {
                Self::try_bind_first_unbound(ctx.type_params, ctx.subst, ty.clone());
            }
            Type::TyVar(name) => {
                if !name.starts_with("α_") && !name.starts_with('@') {
                    Self::try_bind_first_unbound(ctx.type_params, ctx.subst, ty.clone());
                }
            }
            Type::Unit => {}
            _ => {}
        }
    }

    /// Check if a Sum type is a known ADT encoding and try to bind it as a type arg.
    fn try_bind_compound_sum(&self, ty: &Type, ctx: &mut TypeArgTraversal<'_>) -> bool {
        ctx.check_compound_types
            && ctx.depth > 0
            && self.try_match_adt_type(ty).is_some()
            && Self::try_bind_first_unbound(ctx.type_params, ctx.subst, ty.clone())
    }

    /// Check if a Product type is a known record encoding and try to bind it as a type arg.
    fn try_bind_compound_product(&self, ty: &Type, ctx: &mut TypeArgTraversal<'_>) -> bool {
        ctx.check_compound_types
            && self.try_match_record_type(ty).is_some()
            && Self::try_bind_first_unbound(ctx.type_params, ctx.subst, ty.clone())
    }

    /// Recurse into both sides of a binary type constructor to extract type arguments.
    fn extract_binary_type_args(&self, left: &Type, right: &Type, ctx: &mut TypeArgTraversal<'_>) {
        ctx.depth += 1;
        self.extract_type_args_walk(left, ctx);
        self.extract_type_args_walk(right, ctx);
        ctx.depth -= 1;
    }

    /// Try to bind a type to the first unbound type parameter.
    /// Returns true if a binding was made, false if all params were already bound.
    ///
    /// This is a helper to reduce duplication in extract_type_args_into_subst.
    fn try_bind_first_unbound(
        type_params: &[String],
        subst: &mut HashMap<String, Type>,
        ty: Type,
    ) -> bool {
        for param in type_params {
            if !subst.contains_key(param) {
                subst.insert(param.clone(), ty);
                return true;
            }
        }
        false
    }

    // Note: try_match_record_type, encode_record_fields_to_product, and
    // types_structurally_equal are defined in constructors.rs and shared.

    /// Unify a pattern type with a concrete type, building a substitution.
    /// This is a simple unification that handles type variables.
    pub(in crate::elaborate::exprs) fn unify_to_subst(
        &self,
        pattern: &Type,
        concrete: &Type,
        subst: &mut HashMap<String, Type>,
    ) {
        match pattern {
            Type::TyVar(name) => {
                // Type variable: record the binding
                subst.insert(name.clone(), concrete.clone());
            }
            Type::Arrow(p1, p2) => {
                if let Type::Arrow(c1, c2) = concrete {
                    self.unify_to_subst(p1, c1, subst);
                    self.unify_to_subst(p2, c2, subst);
                }
            }
            Type::Product(p1, p2) => {
                if let Type::Product(c1, c2) = concrete {
                    self.unify_to_subst(p1, c1, subst);
                    self.unify_to_subst(p2, c2, subst);
                }
            }
            Type::Sum(p1, p2) => {
                if let Type::Sum(c1, c2) = concrete {
                    self.unify_to_subst(p1, c1, subst);
                    self.unify_to_subst(p2, c2, subst);
                }
            }
            Type::Mu(_, body) => {
                if let Type::Mu(_, c_body) = concrete {
                    self.unify_to_subst(body, c_body, subst);
                }
            }
            Type::Forall(_, body) => {
                if let Type::Forall(_, c_body) = concrete {
                    self.unify_to_subst(body, c_body, subst);
                }
            }
            Type::App(name, args) => {
                if let Type::App(c_name, c_args) = concrete {
                    if name == c_name && args.len() == c_args.len() {
                        for (p, c) in args.iter().zip(c_args.iter()) {
                            self.unify_to_subst(p, c, subst);
                        }
                    }
                }
            }
            Type::Adt(name, type_args, variants) => {
                if let Type::Adt(c_name, c_type_args, c_variants) = concrete {
                    if name == c_name
                        && type_args.len() == c_type_args.len()
                        && variants.len() == c_variants.len()
                    {
                        for (p, c) in type_args.iter().zip(c_type_args.iter()) {
                            self.unify_to_subst(p, c, subst);
                        }
                        for ((_, p), (_, c)) in variants.iter().zip(c_variants.iter()) {
                            self.unify_to_subst(p, c, subst);
                        }
                    }
                }
            }
            // Base types don't contribute to substitution
            _ => {}
        }
    }
}
