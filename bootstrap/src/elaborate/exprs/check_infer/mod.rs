//! Bidirectional type checking: `check(expr, expected)` and `infer(expr)`.

use crate::ast::Expr;
use crate::span::Spanned;
use tungsten_core::{Term, TermSpan, Type};

use super::blocks::LetCont;

use crate::elaborate::env::{ModulePath, PathResolutionError, ResolvedValue};
use crate::elaborate::error::{ElabError, ElabErrorKind};
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

            // Sorry: accepts any expected type (axiom-like hole). Wrapped in
            // its span so the term itself says the author wrote it — the
            // lowering's own holes are bare (ADR 18.9.26g).
            Expr::Sorry(sorry) => Ok(Term::spanned(
                Term::Sorry,
                TermSpan::new(sorry.span.start, sorry.span.end),
            )),

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

            // Numerics against `Int`/`Nat` are expected-type-driven (ADR
            // 14.9.26c): a literal, a negated literal, an arithmetic expression.
            Expr::IntLiteral(..) | Expr::Unary(..) | Expr::Binary(..) => {
                self.check_numeric(expr, expected)
            }
            // Parentheses are transparent to checking as they are to inference,
            // so `(3 - 5)` against `Int` reaches the numeric arms above.
            Expr::Paren(inner, _) => self.check(inner, expected),

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
            // Defensive: the parser aborts before elaboration when it emitted
            // an `Error` placeholder — uncoded by design (ADR 15.8.26b).
            Expr::Error(span) => Err(ElabError::new(
                *span,
                ElabErrorKind::Other("syntax error".to_string()),
            )),
        }
    }

    // `infer_if` / `check_if` / `infer_annot` / `elab_return`: `control.rs`.
}

mod combinators;
mod control;
mod natind;
mod paths;
mod refl;
mod subst;
mod try_expr;
