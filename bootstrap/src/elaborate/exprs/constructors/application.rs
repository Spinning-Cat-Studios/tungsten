//! Constructor application elaboration.
//!
//! Handles constructors with arguments like `Some(x)` or `Cons(h, t)`.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::elaborate::env;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;
use tungsten_core::{Term, Type};

impl<'a> Elaborator<'a> {
    /// Check a constructor application against an expected type.
    /// This allows inferring unused type parameters from context.
    /// For example: `Err(msg)` checked against `Result<Nat, String>` infers T=Nat.
    pub(in crate::elaborate) fn check_constructor_application(
        &mut self,
        name: &str,
        info: &env::ConstructorInfo,
        args: &[Expr],
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        // Validate arity
        self.validate_ctor_arity(name, info.arity, args.len(), span)?;

        // Get type context
        let ctx = self.get_constructor_context(info, span)?;
        let constructor = &ctx.constructors[info.index];

        // Build substitution from expected type
        let substitution =
            self.build_substitution_from_expected(expected, &ctx.type_params, &info.type_name)?;

        // Substitute into field types and check args
        let arg_terms = self.check_args_against_fields(args, &constructor.fields, &substitution)?;

        // Build constructor term
        let value = self.build_product_value(arg_terms);
        self.build_constructor_term(
            value,
            info.index,
            ctx.constructors.len(),
            expected,
            ctx.is_recursive,
        )
    }

    /// Elaborate a constructor application with arguments (e.g., `Some(x)`, `Cons(h, t)`).
    pub(in crate::elaborate) fn elab_constructor_application(
        &mut self,
        name: &str,
        info: &env::ConstructorInfo,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Validate arity
        self.validate_ctor_arity(name, info.arity, args.len(), span)?;

        // Get type context
        let ctx = self.get_constructor_context(info, span)?;
        let constructor = &ctx.constructors[info.index];

        // Infer argument types
        let (arg_terms, arg_types) = self.infer_args(args)?;

        // Infer type arguments from argument types
        let type_args = self.infer_type_args_from_constructor(
            &ctx.type_params,
            &constructor.fields,
            &arg_types,
            span,
        )?;

        // Get the full ADT type with inferred type arguments
        let adt_type = self.encode_adt_type(&info.type_name, &type_args)?;

        // Build constructor term
        let value = self.build_product_value(arg_terms);
        let term = self.build_constructor_term(
            value,
            info.index,
            ctx.constructors.len(),
            &adt_type,
            ctx.is_recursive,
        )?;

        Ok((term, adt_type))
    }

    /// Build a substitution map from an expected ADT type.
    /// Extracts type arguments and adds the self-reference for recursive types.
    pub(crate) fn build_substitution_from_expected(
        &mut self,
        expected: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<HashMap<String, Type>> {
        // Extract type arguments from expected type using unification
        let expected_type_args =
            self.extract_type_args_from_adt_by_unification(expected, type_params, adt_name)?;

        // Build substitution from type parameters
        let mut substitution: HashMap<String, Type> = HashMap::new();
        for (param, ty) in type_params.iter().zip(expected_type_args.iter()) {
            substitution.insert(param.clone(), ty.clone());
        }

        // Add self-reference for recursive types
        substitution.insert(adt_name.to_string(), expected.clone());

        Ok(substitution)
    }

    /// Check arguments against field types with substitution applied.
    pub(crate) fn check_args_against_fields(
        &mut self,
        args: &[Expr],
        field_types: &[Type],
        substitution: &HashMap<String, Type>,
    ) -> ElabResult<Vec<Term>> {
        let concrete_field_types: Vec<Type> = field_types
            .iter()
            .map(|ft| self.substitute_type_vars(ft, substitution))
            .collect();

        let mut arg_terms = Vec::new();
        for (arg, field_ty) in args.iter().zip(concrete_field_types.iter()) {
            let term = self.check(arg, field_ty)?;
            arg_terms.push(term);
        }

        Ok(arg_terms)
    }

    /// Extract type arguments from an ADT type using proper unification.
    ///
    /// This creates a "pattern" type by encoding the ADT with TyVars for each
    /// type parameter, then unifies that pattern with the concrete expected type
    /// to extract the actual type arguments.
    ///
    /// For example, for `List<(Nat, Nat)>`:
    /// - Pattern: `μα_List. (Unit + (TyVar("T") × α_List))`
    /// - Concrete: `μα_List. (Unit + ((Nat × Nat) × α_List))`
    /// - Unify: T = (Nat × Nat)
    ///
    /// Also handles nominal types directly:
    /// - Expected: `App("Option", [String])`
    /// - For ADT "Option" with params ["T"]
    /// - Directly extracts: [String]
    pub(crate) fn extract_type_args_from_adt_by_unification(
        &mut self,
        expected: &Type,
        type_params: &[String],
        adt_name: &str,
    ) -> ElabResult<Vec<Type>> {
        if type_params.is_empty() {
            return Ok(vec![]);
        }

        // Fast path: if expected is Type::App for the same ADT, extract directly
        if let Type::App(name, args) = expected {
            if name == adt_name && args.len() == type_params.len() {
                return Ok(args.clone());
            }
        }

        // Create TyVars for each type parameter to build the pattern type
        let type_param_vars: Vec<Type> =
            type_params.iter().map(|p| Type::TyVar(p.clone())).collect();

        // Encode the ADT with type parameter TyVars as arguments
        // This gives us a pattern like: μα_List. (Unit + (TyVar("T") × α_List))
        let pattern = self.encode_adt_type(adt_name, &type_param_vars)?;

        // Unify the pattern with the concrete expected type to extract bindings
        let mut substitution: HashMap<String, Type> = HashMap::new();
        self.unify_to_subst(&pattern, expected, &mut substitution);

        // Build result in parameter order
        let results: Vec<Type> = type_params
            .iter()
            .map(|p| substitution.get(p).cloned().unwrap_or(Type::Unit))
            .collect();

        Ok(results)
    }

    /// Infer the types of multiple argument expressions.
    pub(crate) fn infer_args(&mut self, args: &[Expr]) -> ElabResult<(Vec<Term>, Vec<Type>)> {
        let mut arg_terms = Vec::new();
        let mut arg_types = Vec::new();
        for arg in args {
            let (term, ty) = self.infer(arg)?;
            arg_terms.push(term);
            arg_types.push(ty);
        }
        Ok((arg_terms, arg_types))
    }

    /// Infer type arguments from constructor argument types.
    /// Given type parameters [T, U], field types [T, List<U>], and arg types [Nat, List<Bool>],
    /// returns [Nat, Bool].
    pub(crate) fn infer_type_args_from_constructor(
        &self,
        type_params: &[String],
        field_types: &[Type],
        arg_types: &[Type],
        _span: Span,
    ) -> ElabResult<Vec<Type>> {
        let mut substitution: HashMap<String, Type> = HashMap::new();

        // Unify each field type with its corresponding argument type
        for (field_ty, arg_ty) in field_types.iter().zip(arg_types.iter()) {
            self.unify_to_subst(field_ty, arg_ty, &mut substitution);
        }

        // Build the result in order of type parameters
        let mut result = Vec::new();
        for param in type_params {
            if let Some(ty) = substitution.get(param) {
                result.push(ty.clone());
            } else {
                // Type parameter wasn't constrained - use Unit as placeholder
                // This shouldn't happen in well-formed code
                result.push(Type::Unit);
            }
        }

        Ok(result)
    }
}
