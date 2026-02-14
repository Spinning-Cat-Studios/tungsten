//! Helper utilities for expression elaboration.
//!
//! Contains:
//! - PatternBinding struct for nested pattern matching
//! - Pattern-to-name extraction
//! - Type substitution utilities

use std::collections::{HashMap, HashSet};

use crate::ast::Pattern;
use crate::span::Spanned;
use tungsten_core::Type;

use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Binding information collected from a nested pattern.
/// Maps variable names to their types (for env binding).
pub(super) struct PatternBinding {
    pub var_name: String,
    pub var_ty: Type,
}

impl<'a> Elaborator<'a> {
    /// Extract variable name from a pattern (only variables supported).
    pub(crate) fn pattern_to_name(&self, pattern: &Pattern) -> ElabResult<String> {
        match pattern {
            Pattern::Var(ident) => Ok(ident.name.clone()),
            Pattern::Wildcard(_) => Ok("_".to_string()),
            _ => Err(ElabError::new(
                pattern.span(),
                ElabErrorKind::UnsupportedPattern("complex patterns".to_string()),
            )
            .with_help("use simple variable patterns in Phase 1")),
        }
    }

    /// Substitute type variables in a type using a substitution map.
    pub(super) fn substitute_type_vars(&self, ty: &Type, subst: &HashMap<String, Type>) -> Type {
        match ty {
            Type::TyVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
            Type::Arrow(param, ret) => Type::arrow(
                self.substitute_type_vars(param, subst),
                self.substitute_type_vars(ret, subst),
            ),
            Type::Product(left, right) => Type::product(
                self.substitute_type_vars(left, subst),
                self.substitute_type_vars(right, subst),
            ),
            Type::Sum(left, right) => Type::sum(
                self.substitute_type_vars(left, subst),
                self.substitute_type_vars(right, subst),
            ),
            Type::Forall(param, body) => {
                // Don't substitute bound variables
                let mut new_subst = subst.clone();
                new_subst.remove(param);
                Type::forall(param.clone(), self.substitute_type_vars(body, &new_subst))
            }
            Type::Mu(param, body) => {
                // Don't substitute bound variables
                let mut new_subst = subst.clone();
                new_subst.remove(param);
                Type::mu(param.clone(), self.substitute_type_vars(body, &new_subst))
            }
            Type::App(name, args) => {
                // Substitute in type arguments
                let subst_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.substitute_type_vars(a, subst))
                    .collect();
                Type::app(name.clone(), subst_args)
            }
            _ => ty.clone(),
        }
    }

    /// Substitute recursive type references in a field type.
    /// E.g., for List<T>, substitute the ADT name with the full μ-type.
    ///
    /// When pattern matching on a recursive ADT like `LexErrors = μα_LexErrors. (Unit + (LexError * α_LexErrors))`,
    /// the constructor field types reference the ADT name (e.g., `TyVar("LexErrors")`) for recursive fields.
    /// We need to substitute this with the full μ-type so that recursive fields have the correct type
    /// for function calls expecting `LexErrors` (which is the μ-type).
    pub(super) fn substitute_recursive_refs(&mut self, ty: &Type, adt_type: &Type) -> Type {
        // If adt_type is μα_Foo.F, the stored field types use TyVar("Foo") for recursive references.
        // We need to substitute TyVar("Foo") with the full μ-type.
        if let Type::Mu(mu_var, _) = adt_type {
            // Extract the ADT name from the μ-variable name (e.g., "LexErrors" from "α_LexErrors")
            let adt_name = if mu_var.starts_with("α_") {
                &mu_var[3..] // Skip "α_" (which is 3 bytes for the UTF-8 α character)
            } else {
                mu_var.as_str()
            };

            // Build substitution: ADT name -> full μ-type
            let mut subst = HashMap::new();
            subst.insert(adt_name.to_string(), adt_type.clone());
            let substituted = self.substitute_type_vars(ty, &subst);

            // After substitution, resolve any Type::App that can now be expanded
            self.resolve_type_apps(&substituted)
        } else {
            // Not a μ-type - still need to resolve any Type::App references
            self.resolve_type_apps(ty)
        }
    }

    /// Resolve Type::App references to their encoded forms.
    ///
    /// This expands deferred type applications like `Type::App("Forest", [Nat])`
    /// to the fully encoded μ-type `μα_Forest. (Unit + ((Nat + (Nat × α_Forest)) × α_Forest))`.
    pub(super) fn resolve_type_apps(&mut self, ty: &Type) -> Type {
        let mut encoding_stack = HashSet::new();
        self.resolve_type_apps_impl(ty, &mut encoding_stack)
    }

    /// Internal implementation of resolve_type_apps with cycle detection.
    fn resolve_type_apps_impl(&mut self, ty: &Type, encoding_stack: &mut HashSet<String>) -> Type {
        match ty {
            Type::App(name, args) if !encoding_stack.contains(name) => {
                // Try to encode the type application
                if let Some(type_def) = self.env.lookup_type(name).cloned() {
                    if !matches!(type_def.kind, TypeDefKind::Stub) {
                        // Resolve arguments first
                        let resolved_args: Vec<Type> = args
                            .iter()
                            .map(|a| self.resolve_type_apps_impl(a, encoding_stack))
                            .collect();

                        // Add to stack before encoding to detect cycles
                        encoding_stack.insert(name.clone());

                        let result = match &type_def.kind {
                            TypeDefKind::ADT(_) => {
                                // Encode the ADT with resolved arguments
                                if let Ok(encoded) =
                                    self.encode_adt_type_impl(name, &resolved_args, encoding_stack)
                                {
                                    encoded
                                } else {
                                    Type::app(name.clone(), resolved_args)
                                }
                            }
                            TypeDefKind::Alias(alias_ty) => {
                                // Substitute type params in the alias
                                let mut result = alias_ty.clone();
                                for (param, arg) in type_def.params.iter().zip(resolved_args.iter())
                                {
                                    result = result.substitute(param, arg);
                                }
                                self.resolve_type_apps_impl(&result, encoding_stack)
                            }
                            TypeDefKind::Record(_) => {
                                // Keep record as nominal type - encoding happens at codegen
                                // Resolve arguments but keep as App
                                Type::app(name.clone(), resolved_args)
                            }
                            TypeDefKind::Stub => Type::app(name.clone(), resolved_args),
                        };

                        encoding_stack.remove(name);
                        return result;
                    }
                }
                // Couldn't resolve - keep as App with resolved args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_apps_impl(a, encoding_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_type_apps_impl(a, encoding_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }
            // Recursively resolve in composite types
            Type::Arrow(a, b) => Type::arrow(
                self.resolve_type_apps_impl(a, encoding_stack),
                self.resolve_type_apps_impl(b, encoding_stack),
            ),
            Type::Product(a, b) => Type::product(
                self.resolve_type_apps_impl(a, encoding_stack),
                self.resolve_type_apps_impl(b, encoding_stack),
            ),
            Type::Sum(a, b) => Type::sum(
                self.resolve_type_apps_impl(a, encoding_stack),
                self.resolve_type_apps_impl(b, encoding_stack),
            ),
            Type::Mu(v, body) => {
                Type::mu(v.clone(), self.resolve_type_apps_impl(body, encoding_stack))
            }
            Type::Forall(v, body) => {
                Type::forall(v.clone(), self.resolve_type_apps_impl(body, encoding_stack))
            }
            Type::Ptr(inner) => Type::ptr(self.resolve_type_apps_impl(inner, encoding_stack)),
            Type::Ref(inner) => Type::ref_ty(self.resolve_type_apps_impl(inner, encoding_stack)),
            // Leaf types - no App to resolve
            _ => ty.clone(),
        }
    }

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
    fn build_type_param_substitution_by_unification(
        &mut self,
        adt_type: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> HashMap<String, Type> {
        let mut subst = HashMap::new();

        if type_params.is_empty() {
            return subst;
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
    pub(super) fn instantiate_constructor_fields(
        &mut self,
        fields: &[Type],
        type_params: &[String],
        adt_type: &Type,
    ) -> Vec<Type> {
        // Extract the ADT name from the μ-type, or empty for non-recursive types
        let adt_name = if let Type::Mu(mu_var, _) = adt_type {
            // Extract name from μ-variable (e.g., "List" from "α_List")
            if mu_var.starts_with("α_") {
                &mu_var[3..] // Skip "α_" (which is 3 bytes for UTF-8 α)
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
    pub(super) fn instantiate_constructor_fields_with_name(
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
    pub(super) fn extract_type_args_into_subst(
        &self,
        ty: &Type,
        type_params: &[String],
        subst: &mut HashMap<String, Type>,
        depth: usize,
        check_compound_types: bool,
    ) {
        match ty {
            Type::Mu(_bound_var, body) => {
                // Skip the recursion variable, increment depth
                self.extract_type_args_into_subst(
                    body,
                    type_params,
                    subst,
                    depth + 1,
                    check_compound_types,
                );
            }
            Type::Sum(left, right) => {
                // At depth > 0, check if this sum type is a known ADT type argument
                // At depth 0, we're at the outer ADT level - don't check (would match self)
                if check_compound_types && depth > 0 {
                    if self.try_match_adt_type(ty).is_some() {
                        // This sum IS an ADT type encoding - use it as the type arg
                        if Self::try_bind_first_unbound(type_params, subst, ty.clone()) {
                            return;
                        }
                    }
                }
                // Recurse into sum components to find type arguments
                self.extract_type_args_into_subst(
                    left,
                    type_params,
                    subst,
                    depth + 1,
                    check_compound_types,
                );
                self.extract_type_args_into_subst(
                    right,
                    type_params,
                    subst,
                    depth + 1,
                    check_compound_types,
                );
            }
            Type::Product(_, _) => {
                // Product might be a record type encoding, a tuple, or part of ADT structure.
                //
                // When check_compound_types=true (pattern matching with nested ADTs),
                // check if it's a known record type first.
                if check_compound_types && self.try_match_record_type(ty).is_some() {
                    // This product IS a record type encoding - use it as the type arg
                    if Self::try_bind_first_unbound(type_params, subst, ty.clone()) {
                        return;
                    }
                }
                // Recurse into product components to find type arguments
                if let Type::Product(left, right) = ty {
                    self.extract_type_args_into_subst(
                        left,
                        type_params,
                        subst,
                        depth + 1,
                        check_compound_types,
                    );
                    self.extract_type_args_into_subst(
                        right,
                        type_params,
                        subst,
                        depth + 1,
                        check_compound_types,
                    );
                }
            }
            Type::Arrow(param, ret) => {
                self.extract_type_args_into_subst(
                    param,
                    type_params,
                    subst,
                    depth + 1,
                    check_compound_types,
                );
                self.extract_type_args_into_subst(
                    ret,
                    type_params,
                    subst,
                    depth + 1,
                    check_compound_types,
                );
            }
            // For concrete leaf types, assign them to type params in order
            // This is a simplification - works when type params appear at leaves
            Type::Nat | Type::Bool | Type::String => {
                // Find first unbound type param and assign this type
                Self::try_bind_first_unbound(type_params, subst, ty.clone());
            }
            Type::TyVar(name) => {
                // A type variable found in the μ-type body IS a type argument.
                // This includes cases where:
                // - It's a concrete type name like "Point"
                // - It's a type parameter from an outer scope (like T in fn foo<T>(x: List<T>))
                //
                // We should NOT skip self-named variables because when List<T> is encoded,
                // the T in the body IS the type argument that should be extracted.
                //
                // Only skip μ-bound variables (like "α_List") which are recursion markers.
                if !name.starts_with("α_") {
                    // This is a type argument - assign it to the first unbound param
                    Self::try_bind_first_unbound(type_params, subst, ty.clone());
                }
            }
            Type::Unit => {
                // Unit is usually not a type parameter instantiation
            }
            _ => {}
        }
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
    pub(super) fn unify_to_subst(
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
            // Base types don't contribute to substitution
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tungsten_core::Context;

    /// Create an Elaborator for testing.
    fn make_elaborator() -> Elaborator<'static> {
        let ctx = Box::leak(Box::new(Context::new()));
        Elaborator::new(ctx)
    }

    // ========================================================================
    // Tests for `extract_type_args_into_subst` (ADR 30.1.26.2 fix)
    // ========================================================================
    //
    // The fix modifies how TyVars are handled during type argument extraction:
    // - TyVars like "T" in a μ-type body ARE type arguments (not skipped)
    // - Only μ-bound vars (α_*) are skipped
    //
    // This enables correct type inference for generic ADT patterns like List<T>

    /// Extract type args from a simple instantiation: List<Nat> body.
    /// Mu("α_List", Sum(Unit, Product(Nat, TyVar("α_List"))))
    /// Should extract [Nat] for type param T.
    #[test]
    fn test_extract_type_args_list_nat() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        // Mu("α_List", Sum(Unit, Product(Nat, TyVar("α_List"))))
        let body = Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("α_List".to_string())),
        );
        let list_nat = Type::mu("α_List", body);

        elab.extract_type_args_into_subst(&list_nat, &type_params, &mut subst, 0, false);

        assert_eq!(subst.get("T"), Some(&Type::Nat));
    }

    /// Extract type args from List<String>.
    #[test]
    fn test_extract_type_args_list_string() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        // Mu("α_List", Sum(Unit, Product(String, TyVar("α_List"))))
        let body = Type::sum(
            Type::Unit,
            Type::product(Type::String, Type::TyVar("α_List".to_string())),
        );
        let list_string = Type::mu("α_List", body);

        elab.extract_type_args_into_subst(&list_string, &type_params, &mut subst, 0, false);

        assert_eq!(subst.get("T"), Some(&Type::String));
    }

    /// Extract type args from a generic List<T> with type variable.
    /// This is the KEY TEST for the fix in ADR 30.1.26.2:
    /// When we have Mu("α_List", Sum(Unit, Product(TyVar("T"), TyVar("α_List")))),
    /// the TyVar("T") SHOULD be extracted as the type argument.
    #[test]
    fn test_extract_type_args_list_generic() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        // Mu("α_List", Sum(Unit, Product(TyVar("T"), TyVar("α_List"))))
        // This represents List<T> from a generic context
        let body = Type::sum(
            Type::Unit,
            Type::product(
                Type::TyVar("T".to_string()), // This should be extracted!
                Type::TyVar("α_List".to_string()),
            ),
        );
        let list_t = Type::mu("α_List", body);

        elab.extract_type_args_into_subst(&list_t, &type_params, &mut subst, 0, false);

        // The fix: TyVar("T") should be extracted as the type argument
        assert_eq!(subst.get("T"), Some(&Type::TyVar("T".to_string())));
    }

    /// μ-bound variable (α_List) should NOT be extracted as type argument.
    #[test]
    fn test_extract_type_args_skips_mu_bound() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        // Just the body: Sum(Unit, Product(TyVar("α_List"), TyVar("α_List")))
        // α_List should NOT be extracted as it's a μ-bound recursion marker
        let body = Type::sum(
            Type::Unit,
            Type::product(
                Type::TyVar("α_List".to_string()),
                Type::TyVar("α_List".to_string()),
            ),
        );

        elab.extract_type_args_into_subst(&body, &type_params, &mut subst, 1, false);

        // T should NOT be bound because α_List is not a valid type arg
        assert_eq!(subst.get("T"), None);
    }

    /// Extract multiple type args: Result<String, Nat>
    #[test]
    fn test_extract_type_args_result() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string(), "E".to_string()];
        let mut subst = HashMap::new();

        // Sum(String, Nat) - Result<String, Nat> encoded
        // Note: Result is non-recursive, so no μ-type
        let result_ty = Type::sum(Type::String, Type::Nat);

        elab.extract_type_args_into_subst(&result_ty, &type_params, &mut subst, 0, false);

        // T -> String, E -> Nat (left-to-right extraction)
        assert_eq!(subst.get("T"), Some(&Type::String));
        assert_eq!(subst.get("E"), Some(&Type::Nat));
    }

    /// Option<Nat>: Sum(Unit, Nat) -> extracts Nat for T
    #[test]
    fn test_extract_type_args_option() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        // Sum(Unit, Nat) - Option<Nat>
        let option_nat = Type::sum(Type::Unit, Type::Nat);

        elab.extract_type_args_into_subst(&option_nat, &type_params, &mut subst, 0, false);

        // T -> Nat (Unit is skipped as not a type param instantiation)
        assert_eq!(subst.get("T"), Some(&Type::Nat));
    }

    /// Unit is not extracted as a type parameter.
    #[test]
    fn test_extract_type_args_unit_skipped() {
        let elab = make_elaborator();
        let type_params = vec!["T".to_string()];
        let mut subst = HashMap::new();

        elab.extract_type_args_into_subst(&Type::Unit, &type_params, &mut subst, 1, false);

        // Unit should not bind T
        assert_eq!(subst.get("T"), None);
    }

    /// Arrow types: (T -> U) should extract T and U.
    #[test]
    fn test_extract_type_args_arrow() {
        let elab = make_elaborator();
        let type_params = vec!["A".to_string(), "B".to_string()];
        let mut subst = HashMap::new();

        // Nat -> String
        let arrow = Type::arrow(Type::Nat, Type::String);

        elab.extract_type_args_into_subst(&arrow, &type_params, &mut subst, 0, false);

        assert_eq!(subst.get("A"), Some(&Type::Nat));
        assert_eq!(subst.get("B"), Some(&Type::String));
    }
}
