//! Block and let binding elaboration.
//!
//! Handles:
//! - `elab_let` - let bindings (including tuple destructuring)
//! - `check_block`/`infer_block` - block expressions
//! - `elab_stmts_then_expr` - statement sequences
//!
//! Tuple destructuring helpers are in `blocks_tuples.rs`.

use crate::ast::{self, Expr, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Continuation for a let-expression: the body to elaborate after
/// the binding, along with expected type and span.
pub(super) struct LetCont<'a> {
    pub body: &'a Expr,
    pub expected: Option<&'a Type>,
    pub span: Span,
}

/// Continuation for a let-statement in a block: the remaining
/// statements, final expression, expected type, and block span.
pub(super) struct StmtLetCont<'a> {
    pub rest: &'a [ast::Stmt],
    pub final_expr: Option<&'a Expr>,
    pub expected: Option<&'a Type>,
    pub span: Span,
}

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
        cont: LetCont,
    ) -> ElabResult<(Term, Type)> {
        // Check for tuple pattern - handle specially
        if let Pattern::Tuple(sub_patterns, pat_span) = pattern {
            return self.elab_let_tuple(sub_patterns, ty_ann, value, cont);
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

        let (body_term, body_ty) = if let Some(expected) = cont.expected {
            let term = self.check(cont.body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(cont.body)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        let term = Term::let_in(name, value_ty, value_term, body_term);
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
            return self.elab_block_final(final_expr, expected, span);
        }

        // Process first statement
        let (first, rest) = stmts.split_first().unwrap();
        match first {
            ast::Stmt::Let(pattern, ty_ann, value, stmt_span) => self.elab_stmt_let_binding(
                pattern,
                ty_ann,
                value,
                StmtLetCont {
                    rest,
                    final_expr,
                    expected,
                    span,
                },
            ),

            ast::Stmt::LetElse(pattern, ty_ann, value, else_expr, stmt_span) => {
                use super::let_else::LetElseArgs;
                let args = LetElseArgs {
                    pattern,
                    ty_ann: ty_ann.as_ref(),
                    value,
                    else_expr,
                };
                self.elab_stmt_let_else(
                    args,
                    StmtLetCont {
                        rest,
                        final_expr,
                        expected,
                        span,
                    },
                    *stmt_span,
                )
            }

            ast::Stmt::Expr(expr, stmt_span) => {
                // Expression statement: evaluate and discard
                let (expr_term, _) = self.infer(expr)?;

                // Best-effort dead-code warning (ADR 13.5.26d §2.7)
                if matches!(expr, Expr::Return(..)) && (!rest.is_empty() || final_expr.is_some()) {
                    let dead_span = if let Some(next) = rest.first() {
                        next.span()
                    } else if let Some(fe) = final_expr {
                        fe.span()
                    } else {
                        *stmt_span
                    };
                    self.warn(
                        ElabError::new(dead_span, ElabErrorKind::DeadCodeAfterReturn)
                            .with_span_note(
                                *stmt_span,
                                "any code after this `return` is unreachable",
                            ),
                    );
                }

                let (body_term, body_ty) =
                    self.elab_stmts_then_expr(rest, final_expr, expected, span)?;

                let term = Term::let_in("_", Type::Unit, expr_term, body_term);
                Ok((term, body_ty))
            }

            ast::Stmt::Item(_item) => Err(ElabError::unsupported(
                first.span(),
                "nested items in blocks",
            )),
        }
    }

    /// Elaborate the final expression (or Unit) at the end of a block.
    fn elab_block_final(
        &mut self,
        final_expr: Option<&Expr>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if let Some(expr) = final_expr {
            if let Some(expected) = expected {
                let term = self.check(expr, expected)?;
                Ok((term, expected.clone()))
            } else {
                self.infer(expr)
            }
        } else {
            if let Some(expected) = expected {
                if !self.types_equal(expected, &Type::Unit) {
                    return Err(ElabError::type_mismatch(span, expected.clone(), Type::Unit));
                }
            }
            Ok((Term::Unit, Type::Unit))
        }
    }

    /// Elaborate a let binding statement, handling tuple and simple patterns.
    fn elab_stmt_let_binding(
        &mut self,
        pattern: &Pattern,
        ty_ann: &Option<crate::ast::TypeExpr>,
        value: &Expr,
        cont: StmtLetCont,
    ) -> ElabResult<(Term, Type)> {
        // Handle tuple patterns specially
        if let Pattern::Tuple(sub_patterns, pat_span) = pattern {
            return self.elab_stmt_let_tuple(sub_patterns, ty_ann.as_ref(), value, cont, *pat_span);
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
            self.elab_stmts_then_expr(cont.rest, cont.final_expr, cont.expected, cont.span)?;

        self.depth -= 1;

        let term = Term::let_in(name, value_ty, value_term, body_term);
        Ok((term, body_ty))
    }
}
