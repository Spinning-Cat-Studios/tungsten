//! Block and let binding elaboration.
//!
//! Handles:
//! - `elab_let` - let bindings (including tuple destructuring)
//! - `check_block`/`infer_block` - block expressions
//! - `elab_stmts_then_expr` - statement sequences

use crate::ast::{self, Expr, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate let binding.
    ///
    /// Supports:
    /// - Simple variable patterns: `let x = e`
    /// - Wildcard patterns: `let _ = e`
    /// - Tuple patterns: `let (a, b) = e` (desugars to nested lets with projections)
    pub(super) fn elab_let(
        &mut self,
        pattern: &Pattern,
        ty_ann: Option<&ast::TypeExpr>,
        value: &Expr,
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Check for tuple pattern - handle specially
        if let Pattern::Tuple(sub_patterns, pat_span) = pattern {
            return self.elab_let_tuple(sub_patterns, ty_ann, value, body, expected, *pat_span);
        }

        // Simple pattern (variable or wildcard)
        let name = self.pattern_to_name(pattern)?;

        // Elaborate the value
        let (value_term, value_ty) = if let Some(ann) = ty_ann {
            let expected_ty = self.elab_type(ann)?;
            let term = self.check(value, &expected_ty)?;
            (term, expected_ty)
        } else {
            self.infer(value)?
        };

        // Bind and elaborate body
        self.env.push_scope();
        self.env
            .bind_local(name.clone(), value_ty.clone(), self.depth);
        self.depth += 1;

        let (body_term, body_ty) = if let Some(expected) = expected {
            let term = self.check(body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(body)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        let term = Term::let_in(name, value_ty, value_term, body_term);
        Ok((term, body_ty))
    }

    /// Elaborate let binding with tuple pattern.
    ///
    /// Desugars `let (a, b) = expr` to:
    /// ```text
    /// let __tup = expr;
    /// let a = fst(__tup);
    /// let b = snd(__tup);
    /// body
    /// ```
    ///
    /// Supports nested tuples like `let (a, (b, c)) = expr`.
    fn elab_let_tuple(
        &mut self,
        sub_patterns: &[Pattern],
        ty_ann: Option<&ast::TypeExpr>,
        value: &Expr,
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Type annotations on tuple patterns are not supported
        if ty_ann.is_some() {
            return Err(ElabError::new(
                span,
                ElabErrorKind::UnsupportedPattern("type annotations on tuple patterns".to_string()),
            )
            .with_help("remove the type annotation; tuple element types are inferred"));
        }

        // Elaborate the value expression
        let (value_term, value_ty) = self.infer(value)?;

        // Verify the value type is a product with the right arity
        let element_types = self.extract_tuple_types(&value_ty, sub_patterns.len(), span)?;

        // Create a fresh variable for the tuple value
        let tuple_var = self.fresh_var("__tup");

        // Push a scope for the bindings
        self.env.push_scope();

        // Bind the tuple variable
        self.env
            .bind_local(tuple_var.clone(), value_ty.clone(), self.depth);
        self.depth += 1;

        // Collect all bindings from the tuple pattern and bind them
        let bindings =
            self.collect_tuple_bindings(sub_patterns, &element_types, &tuple_var, span)?;

        for (name, ty) in &bindings {
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
        }

        // Elaborate the body with all bindings in scope
        let (body_term, body_ty) = if let Some(expected) = expected {
            let term = self.check(body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(body)?
        };

        // Pop depths for bindings
        for _ in &bindings {
            self.depth -= 1;
        }
        self.depth -= 1; // for tuple_var
        self.env.pop_scope();

        // Build nested let bindings for each element (in reverse order)
        let mut result = body_term;
        for (i, (name, ty)) in bindings.iter().enumerate().rev() {
            let projection = self.build_tuple_projection(&tuple_var, i, sub_patterns.len());
            result = Term::let_in(name, ty.clone(), projection, result);
        }

        // Wrap with the outer let for the tuple
        let term = Term::let_in(&tuple_var, value_ty, value_term, result);
        Ok((term, body_ty))
    }

    /// Extract element types from a product type.
    /// For `(A, B, C)` encoded as `A × (B × C)`, returns `[A, B, C]`.
    fn extract_tuple_types(
        &self,
        ty: &Type,
        expected_len: usize,
        span: Span,
    ) -> ElabResult<Vec<Type>> {
        let mut types = Vec::new();
        let mut current = ty.clone();

        for i in 0..expected_len {
            if i == expected_len - 1 {
                // Last element
                types.push(current);
                break;
            }

            match current {
                Type::Product(left, right) => {
                    types.push((*left).clone());
                    current = (*right).clone();
                }
                _ => {
                    return Err(ElabError::new(
                        span,
                        ElabErrorKind::Other(format!(
                            "expected tuple type with {} elements, found non-product type",
                            expected_len
                        )),
                    ));
                }
            }
        }

        Ok(types)
    }

    /// Collect variable bindings from a tuple pattern.
    /// Returns a list of (variable_name, type) pairs in order.
    fn collect_tuple_bindings(
        &self,
        patterns: &[Pattern],
        types: &[Type],
        _tuple_var: &str,
        span: Span,
    ) -> ElabResult<Vec<(String, Type)>> {
        let mut bindings = Vec::new();

        for (pattern, ty) in patterns.iter().zip(types.iter()) {
            self.collect_bindings_from_pattern(pattern, ty, &mut bindings, span)?;
        }

        Ok(bindings)
    }

    /// Recursively collect bindings from a single pattern.
    fn collect_bindings_from_pattern(
        &self,
        pattern: &Pattern,
        ty: &Type,
        bindings: &mut Vec<(String, Type)>,
        span: Span,
    ) -> ElabResult<()> {
        match pattern {
            Pattern::Var(ident) => {
                bindings.push((ident.name.clone(), ty.clone()));
            }
            Pattern::Wildcard(_) => {
                // No binding needed
            }
            Pattern::Tuple(sub_patterns, _) => {
                // Nested tuple - extract element types and recurse
                let element_types = self.extract_tuple_types(ty, sub_patterns.len(), span)?;
                for (sub_pat, elem_ty) in sub_patterns.iter().zip(element_types.iter()) {
                    self.collect_bindings_from_pattern(sub_pat, elem_ty, bindings, span)?;
                }
            }
            _ => {
                return Err(ElabError::new(
                    pattern.span(),
                    ElabErrorKind::UnsupportedPattern(
                        "only variable, wildcard, and tuple patterns are supported in let bindings"
                            .to_string(),
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Build a projection term to extract the i-th element from a tuple.
    /// For `(a, b, c)` encoded as `a × (b × c)`:
    /// - Element 0: `fst(__tup)`
    /// - Element 1: `fst(snd(__tup))`
    /// - Element 2: `snd(snd(__tup))`
    fn build_tuple_projection(&self, tuple_var: &str, index: usize, total: usize) -> Term {
        let mut term = Term::var(tuple_var);

        // Navigate to the right position
        for i in 0..index {
            if i < total - 1 {
                term = Term::snd(term);
            }
        }

        // Extract the element
        if index < total - 1 {
            term = Term::fst(term);
        }
        // Last element is just the remaining snd chain

        term
    }

    /// Elaborate a let statement with tuple pattern in a block.
    /// Similar to `elab_let_tuple` but continues with remaining statements.
    fn elab_stmt_let_tuple(
        &mut self,
        sub_patterns: &[Pattern],
        ty_ann: Option<&ast::TypeExpr>,
        value: &Expr,
        rest_stmts: &[ast::Stmt],
        final_expr: Option<&Expr>,
        expected: Option<&Type>,
        pat_span: Span,
        block_span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Type annotations on tuple patterns are not supported
        if ty_ann.is_some() {
            return Err(ElabError::new(
                pat_span,
                ElabErrorKind::UnsupportedPattern("type annotations on tuple patterns".to_string()),
            )
            .with_help("remove the type annotation; tuple element types are inferred"));
        }

        // Elaborate the value expression
        let (value_term, value_ty) = self.infer(value)?;

        // Verify the value type is a product with the right arity
        let element_types = self.extract_tuple_types(&value_ty, sub_patterns.len(), pat_span)?;

        // Create a fresh variable for the tuple value
        let tuple_var = self.fresh_var("__tup");

        // Bind the tuple variable (no push_scope here - we're already in block scope)
        self.env
            .bind_local(tuple_var.clone(), value_ty.clone(), self.depth);
        self.depth += 1;

        // Collect all bindings from the tuple pattern and bind them
        let bindings =
            self.collect_tuple_bindings(sub_patterns, &element_types, &tuple_var, pat_span)?;

        for (name, ty) in &bindings {
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
        }

        // Continue with the rest of the statements
        let (body_term, body_ty) =
            self.elab_stmts_then_expr(rest_stmts, final_expr, expected, block_span)?;

        // Pop depths for bindings
        for _ in &bindings {
            self.depth -= 1;
        }
        self.depth -= 1; // for tuple_var

        // Build nested let bindings for each element (in reverse order)
        let mut result = body_term;
        for (i, (name, ty)) in bindings.iter().enumerate().rev() {
            let projection = self.build_tuple_projection(&tuple_var, i, sub_patterns.len());
            result = Term::let_in(name, ty.clone(), projection, result);
        }

        // Wrap with the outer let for the tuple
        let term = Term::let_in(&tuple_var, value_ty, value_term, result);
        Ok((term, body_ty))
    }

    /// Elaborate block expression (check mode).
    pub(super) fn check_block(
        &mut self,
        stmts: &[ast::Stmt],
        final_expr: Option<&Expr>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<Term> {
        let (term, _) = self.elab_block_inner(stmts, final_expr, expected, span)?;
        Ok(term)
    }

    /// Elaborate block expression (infer mode).
    pub(super) fn infer_block(
        &mut self,
        stmts: &[ast::Stmt],
        final_expr: Option<&Expr>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        self.elab_block_inner(stmts, final_expr, None, span)
    }

    fn elab_block_inner(
        &mut self,
        stmts: &[ast::Stmt],
        final_expr: Option<&Expr>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        self.env.push_scope();

        // Process statements, building nested lets
        let result = self.elab_stmts_then_expr(stmts, final_expr, expected, span);

        self.env.pop_scope();
        result
    }

    pub(super) fn elab_stmts_then_expr(
        &mut self,
        stmts: &[ast::Stmt],
        final_expr: Option<&Expr>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if stmts.is_empty() {
            // No more statements: elaborate final expression
            if let Some(expr) = final_expr {
                if let Some(expected) = expected {
                    let term = self.check(expr, expected)?;
                    Ok((term, expected.clone()))
                } else {
                    self.infer(expr)
                }
            } else {
                // Empty block: Unit
                if let Some(expected) = expected {
                    if !self.types_equal(expected, &Type::Unit) {
                        return Err(ElabError::type_mismatch(span, expected.clone(), Type::Unit));
                    }
                }
                Ok((Term::Unit, Type::Unit))
            }
        } else {
            // Process first statement
            let (first, rest) = stmts.split_first().unwrap();
            match first {
                ast::Stmt::Let(pattern, ty_ann, value, stmt_span) => {
                    // Handle tuple patterns specially
                    if let Pattern::Tuple(sub_patterns, pat_span) = pattern {
                        return self.elab_stmt_let_tuple(
                            sub_patterns,
                            ty_ann.as_ref(),
                            value,
                            rest,
                            final_expr,
                            expected,
                            *pat_span,
                            span,
                        );
                    }

                    // Simple pattern (variable or wildcard)
                    let name = self.pattern_to_name(pattern)?;

                    // Elaborate value
                    let (value_term, value_ty) = if let Some(ann) = ty_ann {
                        let expected_ty = self.elab_type(ann)?;
                        let term = self.check(value, &expected_ty)?;
                        (term, expected_ty)
                    } else {
                        self.infer(value)?
                    };

                    // Bind and continue
                    self.env
                        .bind_local(name.clone(), value_ty.clone(), self.depth);
                    self.depth += 1;

                    let (body_term, body_ty) =
                        self.elab_stmts_then_expr(rest, final_expr, expected, span)?;

                    self.depth -= 1;

                    let term = Term::let_in(name, value_ty, value_term, body_term);
                    Ok((term, body_ty))
                }

                ast::Stmt::Expr(expr, _) => {
                    // Expression statement: evaluate and discard
                    let (expr_term, _) = self.infer(expr)?;

                    // Continue with rest
                    let (body_term, body_ty) =
                        self.elab_stmts_then_expr(rest, final_expr, expected, span)?;

                    // Use let _ = expr in body (discard result)
                    let term = Term::let_in("_", Type::Unit, expr_term, body_term);
                    Ok((term, body_ty))
                }

                ast::Stmt::Item(_item) => {
                    // Nested items: not supported in Phase 1
                    Err(ElabError::unsupported(
                        first.span(),
                        "nested items in blocks",
                    ))
                }
            }
        }
    }
}
