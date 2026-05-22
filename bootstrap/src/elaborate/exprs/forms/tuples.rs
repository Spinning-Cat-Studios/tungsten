//! Tuple and type application elaboration.
//!
//! Handles:
//! - `elab_tuple` - tuple construction (infer mode)
//! - `check_tuple` - tuple construction (check mode, bidirectional)
//! - `elab_expr_type_app` - explicit type application

use crate::ast::{self, Expr};
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate tuple (infer mode - no expected type).
    pub(in crate::elaborate::exprs) fn elab_tuple(
        &mut self,
        elems: &[Expr],
        _span: Span,
    ) -> ElabResult<(Term, Type)> {
        if elems.is_empty() {
            // Empty tuple = Unit
            return Ok((Term::Unit, Type::Unit));
        }

        if elems.len() == 1 {
            // Single-element "tuple" is just the element
            return self.infer(&elems[0]);
        }

        // Build nested pairs: (a, b, c) → (a, (b, c))
        let mut iter = elems.iter().rev();
        let (mut term, mut ty) = self.infer(iter.next().unwrap())?;

        for elem in iter {
            let (elem_term, elem_ty) = self.infer(elem)?;
            term = Term::pair(elem_term, term);
            ty = Type::product(elem_ty, ty);
        }

        Ok((term, ty))
    }

    /// Check tuple against expected product type (check mode - bidirectional).
    ///
    /// This is key for type inference in patterns like `Cons((a, b), list)`
    /// where the expected type of the tuple is known from the generic context.
    pub(in crate::elaborate::exprs) fn check_tuple(
        &mut self,
        elems: &[Expr],
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        if elems.is_empty() {
            // Empty tuple = Unit
            if !self.types_equal(expected, &Type::Unit) {
                return Err(self.type_mismatch_error(span, expected.clone(), Type::Unit));
            }
            return Ok(Term::Unit);
        }

        if elems.len() == 1 {
            // Single-element "tuple" is just the element
            return self.check(&elems[0], expected);
        }

        // Normalize the expected type to handle type aliases
        let expected_norm = self.normalize_for_comparison(expected);

        // Extract expected types for each element from nested Product types
        // (a, b, c) expects Product(A, Product(B, C))
        let expected_elem_types = self.extract_product_types(&expected_norm, elems.len(), span)?;

        // Check each element against its expected type
        let mut iter = elems.iter().zip(expected_elem_types.iter()).rev();
        let (last_elem, last_ty) = iter.next().unwrap();
        let mut term = self.check(last_elem, last_ty)?;

        for (elem, elem_ty) in iter {
            let elem_term = self.check(elem, elem_ty)?;
            term = Term::pair(elem_term, term);
        }

        Ok(term)
    }

    /// Extract n types from a nested Product type.
    /// Product(A, Product(B, C)) with n=3 → [A, B, C]
    fn extract_product_types(&self, ty: &Type, n: usize, span: Span) -> ElabResult<Vec<Type>> {
        let mut result = Vec::with_capacity(n);
        let mut current = ty.clone();

        for i in 0..n {
            if i == n - 1 {
                // Last element: use the remaining type directly
                result.push(current.clone());
            } else {
                // Extract left type from Product, continue with right
                match &current {
                    Type::Product(left, right) => {
                        result.push((**left).clone());
                        current = (**right).clone();
                    }
                    _ => {
                        return Err(ElabError::new(
                            span,
                            ElabErrorKind::ExpectedType {
                                expected: format!("{}-element tuple type", n),
                                found: ty.clone(),
                            },
                        ));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Elaborate type application.
    pub(in crate::elaborate::exprs) fn elab_expr_type_app(
        &mut self,
        func: &Expr,
        type_args: &[ast::TypeExpr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        let (func_term, func_ty) = self.infer(func)?;

        // Apply type arguments one at a time
        let mut current_term = func_term;
        let mut current_ty = func_ty;

        for type_arg in type_args {
            let Type::Forall(var, body) = current_ty else {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::ExpectedType {
                        expected: "polymorphic type".to_string(),
                        found: current_ty,
                    },
                ));
            };

            let arg_ty = self.elab_type(type_arg)?;
            current_term = Term::TyApp(Box::new(current_term), arg_ty.clone());
            current_ty = body.substitute(&var, &arg_ty);
        }

        Ok((current_term, current_ty))
    }
}
