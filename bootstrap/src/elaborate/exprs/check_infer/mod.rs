//! Bidirectional type checking: `check(expr, expected)` and `infer(expr)`.

use crate::ast::Expr;
use crate::span::Spanned;
use tungsten_core::{Term, TermSpan, Type};

use super::blocks::LetCont;

use crate::elaborate::env::{ModulePath, PathResolutionError, ResolvedValue};
use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Check an expression against an expected type.
    ///
    /// Use this when we know what type the expression should have.
    pub fn check(&mut self, expr: &Expr, expected: &Type) -> ElabResult<Term> {
        // --trace-types instrumentation point 1: check entry (ADR 13.4.26c §5)
        if self.should_trace() {
            self.trace(
                "check entry",
                &format!("expected: {}", self.format_type_with_provenance(expected)),
            );
        }

        match expr {
            // Lambda: if checking against A → B, bind param as A, check body against B
            Expr::Lambda(params, body, span) => self.check_lambda(params, body, expected, *span),

            // If: check condition as Bool, check both branches against expected
            Expr::If(cond, then_branch, else_branch, span) => {
                let term = self.check_if(cond, then_branch, else_branch, expected)?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // Block: elaborate statements, check final expression against expected
            Expr::Block(stmts, final_expr, span) => {
                self.check_block(stmts, final_expr.as_deref(), Some(expected), *span)
            }

            // Let: infer value type, bind, check body against expected
            Expr::Let(pattern, ty_ann, value, body, span) => {
                let (term, _ty) = self.elab_let(
                    pattern,
                    ty_ann.as_ref(),
                    value,
                    LetCont {
                        body,
                        expected: Some(expected),
                        span: *span,
                    },
                )?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // Let-else: desugar to match + diverge
            Expr::LetElse(pattern, ty_ann, value, else_expr, body, span) => {
                use super::let_else::LetElseArgs;
                let args = LetElseArgs {
                    pattern,
                    ty_ann: ty_ann.as_ref(),
                    value,
                    else_expr,
                };
                let (term, _ty) = self.elab_let_else(args, body, Some(expected), *span)?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // If-let: desugar to match (ADR 14.5.26e)
            Expr::IfLet(pattern, init, body, else_branch, span) => {
                use super::if_let::IfLetArgs;
                let args = IfLetArgs {
                    pattern,
                    init,
                    body,
                    else_branch: else_branch.as_deref(),
                };
                let (term, _ty) = self.elab_if_let(args, Some(expected), *span)?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // If-let chain: desugar to nested match/if (ADR 15.5.26d)
            Expr::IfLetChain(conditions, body, else_branch, span) => {
                use super::if_let::IfLetChainArgs;
                let args = IfLetChainArgs {
                    conditions,
                    body,
                    else_branch: else_branch.as_deref(),
                };
                let (term, _ty) = self.elab_if_let_chain(args, Some(expected), *span)?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // Have (proof sugar): have h: P = proof; body
            Expr::Have(name, prop, proof, body, _span) => {
                let (term, _ty) = self.elab_have(name, prop, proof, body, Some(expected))?;
                Ok(term)
            }

            // Show (type ascription): show P { proof }
            Expr::Show(prop, proof, span) => {
                let (term, _ty) = self.elab_show(prop, proof, Some(expected), *span)?;
                Ok(term)
            }

            // Assume (lambda intro): assume h: P; body
            Expr::Assume(name, prop, body, span) => {
                let (term, _ty) = self.elab_assume(name, prop, body, Some(expected), *span)?;
                Ok(term)
            }

            // Match: infer scrutinee, check arms against expected
            Expr::Match(scrutinee, arms, span) => {
                let (term, _ty) = self.elab_match(scrutinee, arms, Some(expected), *span)?;
                Ok(Term::spanned(term, TermSpan::new(span.start, span.end)))
            }

            // Record literal: use expected type to determine field types
            Expr::RecordLit {
                spread,
                fields,
                span,
            } => self.elab_record_literal(spread.as_deref(), fields, expected, *span),

            // Sorry: accepts any expected type (axiom-like hole)
            Expr::Sorry(_) => Ok(Term::Sorry),

            // Refl: check against equality type (ADR 21.5.26d)
            Expr::Refl(span) => self.check_refl(*span, expected),

            // Subst: check against expected type (ADR 21.5.26d, 21.5.26g)
            Expr::Subst(proof, motive, witness, span) => {
                self.check_subst(proof, motive, witness, expected, *span)
            }

            // Constructor: use expected type to determine type arguments
            Expr::Path(path) => self.check_path(path, expected, expr),

            // Constructor application: use expected type to determine type arguments
            Expr::App(func, args, span) => self.check_app(func, args, expected, expr, *span),

            // Tuple: propagate expected type into elements
            Expr::Tuple(elems, span) => self.check_tuple(elems, expected, *span),

            // Return: type is ⊥, which unifies with any expected type
            Expr::Return(inner, span) => {
                let (term, _) = self.elab_return(inner.as_deref(), *span)?;
                Ok(term)
            }

            // Try: expr? — desugar to match + early return
            Expr::Try(inner, span) => {
                let (term, _) = self.elab_try(inner, *span)?;
                Ok(term)
            }

            // Try block: try { body } — desugar to checked IIFE (ADR 15.5.26d)
            Expr::TryBlock(body, span) => {
                let (term, _) = self.elab_try_block(body, Some(expected), *span)?;
                Ok(term)
            }

            // Default: infer type, check it matches expected
            _ => {
                let (term, inferred) = self.infer(expr)?;
                if !self.types_equal(&inferred, expected) {
                    return Err(self.type_mismatch_error(expr.span(), expected.clone(), inferred));
                }
                Ok(term)
            }
        }
    }

    /// Infer the type of an expression.
    ///
    /// Use this when we don't know what type to expect.
    /// Returns both the elaborated term and its type.
    pub fn infer(&mut self, expr: &Expr) -> ElabResult<(Term, Type)> {
        let result = self.infer_inner(expr)?;

        // --trace-types instrumentation point 2: infer exit (ADR 13.4.26c §5)
        if self.should_trace() {
            self.trace(
                "infer exit",
                &format!("inferred: {}", self.format_type_with_provenance(&result.1)),
            );
        }

        Ok(result)
    }

    /// Inner implementation of infer (separated for trace instrumentation).
    fn infer_inner(&mut self, expr: &Expr) -> ElabResult<(Term, Type)> {
        match expr {
            // Literals
            Expr::IntLiteral(n, _span) => Ok((self.nat_literal(*n), Type::Nat)),
            Expr::BoolLiteral(b, _span) => {
                Ok((if *b { Term::True } else { Term::False }, Type::Bool))
            }
            Expr::Unit(_span) => Ok((Term::Unit, Type::Unit)),
            Expr::StringLiteral(s, _span) => Ok((Term::string_lit(s.clone()), Type::String)),

            // Variables
            Expr::Path(path) => self.infer_path(path),

            // Lambda
            Expr::Lambda(params, body, span) => self.infer_lambda(params, body, *span),

            // Application
            // ─────────────────────────────────────────────────────────────────
            Expr::App(func, args, span) => {
                let (term, ty) = self.elab_application(func, args, *span)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }

            // Operators
            Expr::Binary(left, op, right, span) => self.elab_binary(left, *op, right, *span),
            Expr::Unary(op, operand, span) => self.elab_unary(*op, operand, *span),

            // Bindings and control flow
            Expr::Let(pattern, ty_ann, value, body, span) => {
                let (term, ty) = self.elab_let(
                    pattern,
                    ty_ann.as_ref(),
                    value,
                    LetCont {
                        body,
                        expected: None,
                        span: *span,
                    },
                )?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::LetElse(pattern, ty_ann, value, else_expr, body, span) => {
                use super::let_else::LetElseArgs;
                let args = LetElseArgs {
                    pattern,
                    ty_ann: ty_ann.as_ref(),
                    value,
                    else_expr,
                };
                let (term, ty) = self.elab_let_else(args, body, None, *span)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::IfLet(pattern, init, body, else_branch, span) => {
                use super::if_let::IfLetArgs;
                let args = IfLetArgs {
                    pattern,
                    init,
                    body,
                    else_branch: else_branch.as_deref(),
                };
                let (term, ty) = self.elab_if_let(args, None, *span)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::IfLetChain(conditions, body, else_branch, span) => {
                use super::if_let::IfLetChainArgs;
                let args = IfLetChainArgs {
                    conditions,
                    body,
                    else_branch: else_branch.as_deref(),
                };
                let (term, ty) = self.elab_if_let_chain(args, None, *span)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::If(cond, then_branch, else_branch, span) => {
                let (term, ty) = self.infer_if(cond, then_branch, else_branch)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::Match(scrutinee, arms, span) => {
                let (term, ty) = self.elab_match(scrutinee, arms, None, *span)?;
                Ok((Term::spanned(term, TermSpan::new(span.start, span.end)), ty))
            }
            Expr::Block(stmts, final_expr, span) => {
                self.infer_block(stmts, final_expr.as_deref(), *span)
            }

            // Structural
            Expr::Tuple(elems, span) => self.elab_tuple(elems, *span),
            Expr::Annot(inner, ty, _span) => self.infer_annot(inner, ty),
            Expr::TypeApp(func, type_args, span) => self.elab_expr_type_app(func, type_args, *span),

            // Proof constructs
            Expr::Have(name, prop, proof, body, _span) => {
                self.elab_have(name, prop, proof, body, None)
            }
            Expr::Show(prop, proof, span) => self.elab_show(prop, proof, None, *span),
            Expr::Assume(name, prop, body, span) => self.elab_assume(name, prop, body, None, *span),
            Expr::Refl(span) => Err(ElabError::cannot_infer(*span)
                .with_help("add type annotation: `refl : Eq<T, x, x>`")),
            Expr::Subst(proof, motive, witness, span) => {
                self.infer_subst(proof, motive, witness, *span)
            }
            Expr::Sym(proof, span) => self.infer_sym(proof, *span),
            Expr::Trans(h1, h2, span) => self.infer_trans(h1, h2, *span),
            Expr::Cong(f, proof, span) => self.infer_cong(f, proof, *span),
            Expr::NatInd(motive, base, step, n, span) => {
                self.infer_natind(motive, base, step, n, *span)
            }
            Expr::NatRec(result_ty, base, step, n, span) => {
                self.infer_natrec(result_ty, base, step, n, *span)
            }
            Expr::Sorry(sorry) => {
                Err(ElabError::cannot_infer(sorry.span)
                    .with_help("add type annotation: `sorry : T`"))
            }

            // Records, fields, misc
            Expr::RecordLit { span, .. } => Err(ElabError::cannot_infer(*span)
                .with_help("add type annotation: `{ x: 1, y: 2 } : Point`")),
            Expr::NamedRecord {
                name,
                spread,
                fields,
                span,
            } => self.infer_named_record(name, spread.as_deref(), fields, *span),
            Expr::Field(base, field, span) => self.elab_field_access(base, field, *span),
            Expr::Return(inner, span) => self.elab_return(inner.as_deref(), *span),
            Expr::Try(inner, span) => self.elab_try(inner, *span),
            Expr::TryBlock(body, span) => self.elab_try_block(body, None, *span),
            Expr::Paren(inner, _span) => self.infer(inner),
            Expr::Error(span) => Err(ElabError::new(
                *span,
                ElabErrorKind::Other("syntax error".to_string()),
            )),
        }
    }

    /// Infer the type of an if-then-else expression.
    fn infer_if(
        &mut self,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
    ) -> ElabResult<(Term, Type)> {
        let cond_term = self.check(cond, &Type::Bool)?;
        let (then_term, then_ty) = self.infer(then_branch)?;
        // Push context so errors in else branch reference the then branch
        self.push_context(ExpectedContext::branch_unification(then_branch.span()));
        let else_term = self.check(else_branch, &then_ty)?;
        self.pop_context();
        Ok((Term::if_then_else(cond_term, then_term, else_term), then_ty))
    }

    /// Check an if-then-else against an expected type.
    fn check_if(
        &mut self,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        expected: &Type,
    ) -> ElabResult<Term> {
        let cond_term = self.check(cond, &Type::Bool)?;
        let then_term = self.check(then_branch, expected)?;
        let else_term = self.check(else_branch, expected)?;
        Ok(Term::if_then_else(cond_term, then_term, else_term))
    }

    /// Infer the type of an annotated expression.
    fn infer_annot(&mut self, inner: &Expr, ty: &crate::ast::TypeExpr) -> ElabResult<(Term, Type)> {
        let expected = self.elab_type(ty)?;
        let term = self.check(inner, &expected)?;
        Ok((term, expected))
    }

    /// Elaborate a `return` expression (ADR 13.5.26d).
    ///
    /// - `return e` checks `e` against the current function's return type
    /// - bare `return` is `return ()`, valid only when return type is Unit
    /// - Type of `return e` is ⊥ (Void)
    fn elab_return(
        &mut self,
        inner: Option<&Expr>,
        span: crate::span::Span,
    ) -> ElabResult<(Term, Type)> {
        let ret_ty = match &self.current_return_type {
            Some(ty) => ty.clone(),
            None => {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other("return outside of a function body".to_string()),
                ));
            }
        };

        let inner_term = if let Some(expr) = inner {
            self.check(expr, &ret_ty)?
        } else {
            // Bare `return` — only valid when return type is Unit
            if ret_ty != Type::Unit {
                return Err(self.type_mismatch_error(span, ret_ty, Type::Unit));
            }
            Term::Unit
        };

        Ok((Term::early_return(inner_term), Type::Void))
    }
}

mod combinators;
mod natind;
mod paths;
mod refl;
mod subst;
mod try_expr;
