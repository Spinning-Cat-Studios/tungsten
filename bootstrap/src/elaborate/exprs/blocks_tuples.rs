//! Tuple destructuring in let bindings.
//!
//! Handles desugaring of `let (a, b) = expr` into nested let bindings
//! with projections.

use crate::ast::{self, Expr, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

use super::blocks::{LetCont, StmtLetCont};

impl<'a> Elaborator<'a> {
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
    pub(super) fn elab_let_tuple(
        &mut self,
        sub_patterns: &[Pattern],
        ty_ann: Option<&ast::TypeExpr>,
        value: &Expr,
        cont: LetCont,
    ) -> ElabResult<(Term, Type)> {
        // Type annotations on tuple patterns are not supported
        if ty_ann.is_some() {
            return Err(ElabError::new(
                cont.span,
                ElabErrorKind::UnsupportedPattern("type annotations on tuple patterns".to_string()),
            )
            .with_help("remove the type annotation; tuple element types are inferred"));
        }

        // Elaborate the value expression
        let (value_term, value_ty) = self.infer(value)?;

        // Verify the value type is a product with the right arity
        let element_types = self.extract_tuple_types(&value_ty, sub_patterns.len(), cont.span)?;

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
            self.collect_tuple_bindings(sub_patterns, &element_types, &tuple_var, cont.span)?;

        for (name, ty) in &bindings {
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
        }

        // Elaborate the body with all bindings in scope
        let (body_term, body_ty) = if let Some(expected) = cont.expected {
            let term = self.check(cont.body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(cont.body)?
        };

        // Pop depths for bindings
        for _ in &bindings {
            self.depth -= 1;
        }
        self.depth -= 1; // for tuple_var
        self.env.pop_scope();

        // Build nested let bindings that correctly handle wildcards and nesting
        let result = self.build_tuple_lets(
            sub_patterns,
            &element_types,
            &tuple_var,
            body_term,
            cont.span,
        )?;

        // Wrap with the outer let for the tuple
        let term = Term::let_in(&tuple_var, value_ty, value_term, result);
        Ok((term, body_ty))
    }

    /// Extract element types from a product type.
    /// For `(A, B, C)` encoded as `A × (B × C)`, returns `[A, B, C]`.
    pub(super) fn extract_tuple_types(
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

    /// Collect variable bindings from a tuple pattern (flat list for scope registration).
    /// Returns (name, type) pairs for all bound variables, including nested ones.
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
    pub(super) fn collect_bindings_from_pattern(
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

    /// Build nested let-bindings from a tuple pattern, handling wildcards and
    /// nested tuples correctly. Each nesting level generates a fresh intermediate
    /// variable, and projections use the pattern position (not the binding index).
    pub(super) fn build_tuple_lets(
        &mut self,
        patterns: &[Pattern],
        element_types: &[Type],
        tuple_var: &str,
        body: Term,
        span: Span,
    ) -> ElabResult<Term> {
        let total = patterns.len();
        let mut result = body;

        for (i, (pat, ty)) in patterns.iter().zip(element_types.iter()).enumerate().rev() {
            match pat {
                Pattern::Var(ident) => {
                    let proj = self.build_tuple_projection(tuple_var, i, total);
                    result = Term::let_in(&ident.name, ty.clone(), proj, result);
                }
                Pattern::Wildcard(_) => {
                    // No binding needed — skip this position
                }
                Pattern::Tuple(sub_patterns, _) => {
                    let inner_types = self.extract_tuple_types(ty, sub_patterns.len(), span)?;
                    let inner_var = self.fresh_var("__tup");
                    result = self.build_tuple_lets(
                        sub_patterns,
                        &inner_types,
                        &inner_var,
                        result,
                        span,
                    )?;
                    let proj = self.build_tuple_projection(tuple_var, i, total);
                    result = Term::let_in(&inner_var, ty.clone(), proj, result);
                }
                _ => {
                    // Already validated in collect_bindings_from_pattern
                }
            }
        }

        Ok(result)
    }

    /// Build a projection term to extract the i-th element from a tuple.
    /// For `(a, b, c)` encoded as `a × (b × c)`:
    /// - Element 0: `fst(__tup)`
    /// - Element 1: `fst(snd(__tup))`
    /// - Element 2: `snd(snd(__tup))`
    pub(super) fn build_tuple_projection(
        &self,
        tuple_var: &str,
        index: usize,
        total: usize,
    ) -> Term {
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
    pub(super) fn elab_stmt_let_tuple(
        &mut self,
        sub_patterns: &[Pattern],
        ty_ann: Option<&ast::TypeExpr>,
        value: &Expr,
        cont: StmtLetCont,
        pat_span: Span,
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
            self.elab_stmts_then_expr(cont.rest, cont.final_expr, cont.expected, cont.span)?;

        // Pop depths for bindings
        for _ in &bindings {
            self.depth -= 1;
        }
        self.depth -= 1; // for tuple_var

        // Build nested let bindings that correctly handle wildcards and nesting
        let result = self.build_tuple_lets(
            sub_patterns,
            &element_types,
            &tuple_var,
            body_term,
            pat_span,
        )?;

        // Wrap with the outer let for the tuple
        let term = Term::let_in(&tuple_var, value_ty, value_term, result);
        Ok((term, body_ty))
    }
}
