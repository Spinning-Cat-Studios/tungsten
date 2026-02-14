//! Function application elaboration.
//!
//! Handles:
//! - `elab_application` - regular function application
//! - `instantiate_polymorphic_function` - type argument inference
//! - `extract_type_var_bindings` - unification for type inference

use std::collections::HashMap;

use crate::ast::Expr;
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate function application.
    pub(super) fn elab_application(
        &mut self,
        func: &Expr,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Check if this is a built-in function (Phase 3-Prep)
        if let Expr::Path(path) = func {
            if path.is_simple() {
                match path.item_name().name.as_str() {
                    "ref" => return self.elab_ref_new(args, span),
                    "get" => return self.elab_ref_get(args, span),
                    "set" => return self.elab_ref_set(args, span),
                    "char_at" => return self.elab_char_at(args, span),
                    "string_len" => return self.elab_string_len(args, span),
                    "substring" => return self.elab_substring(args, span),
                    _ => {}
                }
            }
        }

        // Check if this is a constructor application (both simple and qualified paths)
        if let Expr::Path(path) = func {
            let ident = path.item_name();
            // Use resolve_constructor_path for both simple and qualified paths
            if let Ok(Some(info)) = self
                .env
                .resolve_constructor_path(path, &self.current_module)
            {
                let info = info.clone();
                // Check constructor visibility
                if !self
                    .env
                    .is_constructor_accessible(&info, &self.current_module, true)
                {
                    if let Some(item_module) = self.env.get_item_module(&info.type_name) {
                        return Err(ElabError::private_item(
                            path.span,
                            &ident.name,
                            "constructor",
                            item_module.to_string(),
                            self.current_module.to_string(),
                        ));
                    }
                }
                // This is a constructor being applied to arguments
                return self.elab_constructor_application(&ident.name, &info, args, span);
            }
        }

        // Regular function application
        let (func_term, func_ty) = self.infer(func)?;

        // If the function is polymorphic, try to instantiate it based on argument types
        let (instantiated_term, instantiated_ty) =
            self.instantiate_polymorphic_function(func_term, func_ty, args, span)?;

        // Apply arguments one at a time
        let mut current_term = instantiated_term;
        let mut current_ty = instantiated_ty;

        for (position, arg) in args.iter().enumerate() {
            let Type::Arrow(param_ty, result_ty) = current_ty else {
                return Err(ElabError::expected_function(arg.span(), current_ty));
            };

            // Push context for better error messages on argument type mismatches
            self.push_context(ExpectedContext::function_arg(position, arg.span()));
            let arg_term = self.check(arg, &param_ty)?;
            self.pop_context();

            current_term = Term::app(current_term, arg_term);
            current_ty = *result_ty;
        }

        Ok((current_term, current_ty))
    }

    /// Instantiate a polymorphic function type based on argument types.
    ///
    /// Given a function `f : forall T. T -> T` and call `f(42)`, this will:
    /// 1. Strip off foralls to get `T -> T` with bound var `T`
    /// 2. Infer arguments to get their types (e.g., `42 : Nat`)
    /// 3. Match parameter type `T` against argument type `Nat` to get `T = Nat`
    /// 4. Apply type application `f[Nat]` and return `f[Nat] : Nat -> Nat`
    pub(super) fn instantiate_polymorphic_function(
        &mut self,
        func_term: Term,
        func_ty: Type,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Collect all forall-bound type variables
        let mut type_vars: Vec<String> = Vec::new();
        let mut inner_ty = func_ty.clone();

        while let Type::Forall(var, body) = inner_ty {
            type_vars.push(var);
            inner_ty = *body;
        }

        // If no foralls, return unchanged
        if type_vars.is_empty() {
            return Ok((func_term, func_ty));
        }

        // We have type variables to instantiate. Infer argument types to guide instantiation.
        let mut arg_types: Vec<Type> = Vec::new();
        for arg in args {
            let (_, arg_ty) = self.infer(arg)?;
            arg_types.push(arg_ty);
        }

        // Build a substitution by matching parameter types against argument types
        let mut subst: HashMap<String, Type> = HashMap::new();

        // Walk through the inner type (after stripping foralls) and match with arg types
        let mut param_ty = inner_ty.clone();
        for arg_ty in &arg_types {
            if let Type::Arrow(expected_param, result) = param_ty {
                // Try to extract type variable bindings from matching expected_param with arg_ty
                self.extract_type_var_bindings(&expected_param, arg_ty, &type_vars, &mut subst);
                param_ty = *result;
            } else {
                break;
            }
        }

        // Check if we got all type variables - if not, this is an error for greedy instantiation
        for var in &type_vars {
            if !subst.contains_key(var) {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::CannotInferTypeArg(var.clone()),
                ));
            }
        }

        // Apply type applications in order
        let mut result_term = func_term;
        let mut result_ty = func_ty;

        for var in &type_vars {
            let ty_arg = subst.get(var).unwrap();
            result_term = Term::TyApp(Box::new(result_term), ty_arg.clone());

            // Unwrap one forall and substitute
            if let Type::Forall(v, body) = result_ty {
                assert_eq!(&v, var);
                result_ty = body.substitute(&v, ty_arg);
            }
        }

        Ok((result_term, result_ty))
    }

    /// Extract type variable bindings by matching expected type against actual type.
    ///
    /// For example, matching `T` against `Nat` gives `T = Nat`.
    /// Matching `Option<T>` against `Option<String>` gives `T = String`.
    pub(super) fn extract_type_var_bindings(
        &self,
        expected: &Type,
        actual: &Type,
        type_vars: &[String],
        subst: &mut HashMap<String, Type>,
    ) {
        match (expected, actual) {
            // If expected is a type variable we're looking for, bind it
            (Type::TyVar(v), actual) if type_vars.contains(v) => {
                // Only bind if not already bound (first binding wins)
                if !subst.contains_key(v) {
                    subst.insert(v.clone(), actual.clone());
                }
            }

            // Recurse into structural types
            (Type::Arrow(e1, e2), Type::Arrow(a1, a2)) => {
                self.extract_type_var_bindings(e1, a1, type_vars, subst);
                self.extract_type_var_bindings(e2, a2, type_vars, subst);
            }
            (Type::Product(e1, e2), Type::Product(a1, a2)) => {
                self.extract_type_var_bindings(e1, a1, type_vars, subst);
                self.extract_type_var_bindings(e2, a2, type_vars, subst);
            }
            (Type::Sum(e1, e2), Type::Sum(a1, a2)) => {
                self.extract_type_var_bindings(e1, a1, type_vars, subst);
                self.extract_type_var_bindings(e2, a2, type_vars, subst);
            }

            // For mu types, try to extract from the body
            (Type::Mu(v1, body1), Type::Mu(v2, body2)) if v1 == v2 => {
                self.extract_type_var_bindings(body1, body2, type_vars, subst);
            }

            // For forall types (nested), try to extract from body
            (Type::Forall(v1, body1), Type::Forall(v2, body2)) if v1 == v2 => {
                self.extract_type_var_bindings(body1, body2, type_vars, subst);
            }

            // Base types and non-matching structures: nothing to extract
            _ => {}
        }
    }
}
