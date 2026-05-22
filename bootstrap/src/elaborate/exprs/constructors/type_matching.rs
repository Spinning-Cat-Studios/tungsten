//! Type matching utilities for constructor elaboration.
//!
//! Provides utilities to match types against known ADT and record encodings,
//! and to detect free type variables.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::{ElabResult, Elaborator};
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Try to match a type against known record type encodings.
    /// Returns the record type name if a match is found.
    pub(in crate::elaborate) fn try_match_record_type(&self, ty: &Type) -> Option<String> {
        for (name, type_def) in self.env.iter_types() {
            if let TypeDefKind::Record(fields) = &type_def.kind {
                let encoded = self.encode_record_type(fields);
                if self.types_structurally_equal(&encoded, ty) {
                    return Some(name.clone());
                }
            }
        }
        None
    }

    /// Try to match a type against known ADT (sum type) encodings.
    /// Returns the ADT type name if a match is found.
    /// This is used to detect when a sum type is a type argument rather than
    /// part of the container ADT's structure.
    ///
    /// For generic ADTs like Option<T>, we match structurally allowing type
    /// variables to match any type. For example, (Unit + Nat) matches Option<T>
    /// because (Unit + T) can unify with (Unit + Nat) by binding T = Nat.
    ///
    /// IMPORTANT: We do NOT match types that contain free type variables (like TyVar("T"))
    /// because those are not fully-instantiated ADTs - they're generic parameters.
    pub(in crate::elaborate) fn try_match_adt_type(&self, ty: &Type) -> Option<String> {
        // Don't match if the type contains free type variables (excluding μ-bound vars)
        // Free type vars indicate this is part of a generic context, not a concrete ADT
        if self.contains_free_type_vars(ty) {
            return None;
        }

        for (name, type_def) in self.env.iter_types() {
            if let TypeDefKind::ADT(constructors) = &type_def.kind {
                // Encode the ADT as a sum type (may contain type variables for generic ADTs)
                let encoded = self.encode_adt_constructors_to_sum(constructors);
                // Use pattern matching that allows type variables to match anything
                if self.types_pattern_match(&encoded, ty, &type_def.params) {
                    return Some(name.clone());
                }
            }
        }
        None
    }

    /// Check if a type contains free type variables (excluding μ-bound variables).
    /// Free type variables like TyVar("T") indicate generic contexts.
    pub(crate) fn contains_free_type_vars(&self, ty: &Type) -> bool {
        self.contains_free_type_vars_impl(ty, &mut HashSet::new())
    }

    fn contains_free_type_vars_impl(&self, ty: &Type, bound: &mut HashSet<String>) -> bool {
        match ty {
            Type::TyVar(name) => {
                // μ-bound variables (α_*) and named types (@*) are not free type vars
                // Regular type variables (T, A, etc.) are free type vars
                !name.starts_with("α_") && !name.starts_with('@') && !bound.contains(name)
            }
            Type::Mu(var, body) => {
                bound.insert(var.clone());
                let result = self.contains_free_type_vars_impl(body, bound);
                bound.remove(var);
                result
            }
            Type::Forall(var, body) => {
                bound.insert(var.clone());
                let result = self.contains_free_type_vars_impl(body, bound);
                bound.remove(var);
                result
            }
            Type::Sum(left, right) | Type::Product(left, right) | Type::Arrow(left, right) => {
                self.contains_free_type_vars_impl(left, bound)
                    || self.contains_free_type_vars_impl(right, bound)
            }
            Type::App(_, args) => args
                .iter()
                .any(|a| self.contains_free_type_vars_impl(a, bound)),
            _ => false,
        }
    }

    /// Check if a pattern type matches a concrete type, allowing type variables
    /// in the pattern to match any type in the concrete.
    ///
    /// This is used by `try_match_adt_type` to check if a concrete type matches
    /// an ADT's structure when the ADT has type parameters.
    pub(crate) fn types_pattern_match(
        &self,
        pattern: &Type,
        concrete: &Type,
        type_params: &[String],
    ) -> bool {
        match (pattern, concrete) {
            // Type variable in pattern matches anything
            (Type::TyVar(name), _) if type_params.contains(name) => true,
            // Same type variables must match
            (Type::TyVar(n1), Type::TyVar(n2)) => n1 == n2,

            // Binary structural matching
            (Type::Sum(p1, p2), Type::Sum(c1, c2))
            | (Type::Product(p1, p2), Type::Product(c1, c2))
            | (Type::Arrow(p1, p2), Type::Arrow(c1, c2)) => {
                self.types_pattern_match(p1, c1, type_params)
                    && self.types_pattern_match(p2, c2, type_params)
            }

            // Binding types: match body structurally
            (Type::Mu(_, pb), Type::Mu(_, cb)) | (Type::Forall(_, pb), Type::Forall(_, cb)) => {
                self.types_pattern_match(pb, cb, type_params)
            }

            // App: must have same name and matching args
            (Type::App(pn, pa), Type::App(cn, ca)) => {
                pn == cn
                    && pa.len() == ca.len()
                    && pa
                        .iter()
                        .zip(ca.iter())
                        .all(|(p, c)| self.types_pattern_match(p, c, type_params))
            }

            // Ref types must match their inner types
            (Type::Ref(p), Type::Ref(c)) => self.types_pattern_match(p, c, type_params),

            // Base types must match exactly
            (Type::Nat, Type::Nat)
            | (Type::Bool, Type::Bool)
            | (Type::String, Type::String)
            | (Type::Unit, Type::Unit)
            | (Type::Void, Type::Void)
            | (Type::Prop, Type::Prop) => true,

            _ => false,
        }
    }

    /// Check if two types are structurally equal.
    /// Handles Type::App by expanding to the encoded form for comparison.
    ///
    /// NOTE: This delegates to `types_structurally_equal_normalized` in
    /// `types/normalize.rs`. All structural type comparisons should go through
    /// the centralized normalization routine. See ADR 25.1.26 Issue 3.
    pub(crate) fn types_structurally_equal(&self, a: &Type, b: &Type) -> bool {
        self.types_structurally_equal_normalized(a, b)
    }

    /// Extract type arguments from an ADT type.
    /// Given expected type and the type definition, unify to extract type arguments.
    /// For example, given `Result<Nat, String>` (encoded as μ...) and params [T, E],
    /// extract [Nat, String].
    #[allow(dead_code)]
    pub(crate) fn extract_type_args_from_adt(
        &self,
        expected: &Type,
        type_params: &[String],
    ) -> ElabResult<Vec<Type>> {
        if type_params.is_empty() {
            return Ok(vec![]);
        }

        // Build a pattern type with type variables for each parameter
        // Then unify with expected to extract the concrete types
        let mut substitution: HashMap<String, Type> = HashMap::new();

        // Use the canonical implementation from helpers.rs
        // depth=0 means we're at the outer ADT level
        // check_compound_types=false: don't try to match Sum/Product as ADT/record types
        // (this is simple extraction, not pattern matching with nested ADTs)
        self.extract_type_args_into_subst(expected, type_params, &mut substitution, 0, false);

        // Build result in parameter order
        let results: Vec<Type> = type_params
            .iter()
            .map(|p| substitution.get(p).cloned().unwrap_or(Type::Unit))
            .collect();

        Ok(results)
    }
}
