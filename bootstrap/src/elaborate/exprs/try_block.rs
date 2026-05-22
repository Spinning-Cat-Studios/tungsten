//! Elaboration of `try { body }` blocks (ADR 15.5.26d).
//!
//! A `try` block desugars to a checked IIFE (immediately-invoked function expression):
//!
//! ```text
//! try { body }  →  (fn() => Ok(body))()   [elaborated in check mode]
//! ```
//!
//! The key insight from the P3 proof fixtures: the lambda body MUST be elaborated
//! in check mode (via `with_return_context(Some(result_ty))`) so that `?` inside
//! the body sees the correct return type. Source-level IIFE lambdas use infer mode,
//! which clears the return context — so the desugaring must be done at the
//! elaboration level, not as a syntactic transformation.
//!
//! ## Return rejection
//!
//! Explicit `return` inside a try block is rejected. The elaborator scans the body
//! AST for `Return` nodes before desugaring and emits `ReturnInsideTryBlock`.

use crate::ast::{Expr, Motive};
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate a `try { body }` block.
    ///
    /// In check mode (expected type known), the expected type must be `Result<T, E>`.
    /// In infer mode, we require the body to contain at least one `?` usage so the
    /// error type `E` can be inferred — otherwise we emit a cannot-infer error.
    pub(super) fn elab_try_block(
        &mut self,
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Reject explicit `return` inside the try body
        if let Some(ret_span) = find_return_in_expr(body) {
            return Err(ElabError::new(
                ret_span,
                ElabErrorKind::ReturnInsideTryBlock,
            ));
        }

        // 2. Determine the Result<T, E> type we're targeting
        let result_ty = match expected {
            Some(ty) => {
                // Verify the expected type is structurally a Result (Sum type)
                if !self.type_matches_adt(ty, "Result") {
                    return Err(ElabError::new(
                        span,
                        ElabErrorKind::TypeMismatch {
                            expected: ty.clone(),
                            found: Type::Unit, // placeholder
                        },
                    )
                    .with_help("try block must evaluate to Result<T, E>"));
                }
                ty.clone()
            }
            None => {
                // Infer mode: elaborate the body with a temporary return context,
                // then wrap in Ok. We need to infer T from the body and E from `?` usage.
                // For now, elaborate the body and construct the result type.
                //
                // We elaborate the body in a return context where the return type
                // is unknown. The `?` operator will set up the error type.
                // If no `?` is used, we'll get TryOutsideReturnContext errors,
                // which is the correct behavior.
                return self.elab_try_block_infer(body, span);
            }
        };

        // 3. Extract T and E from the Result<T, E> type
        let (ok_type, _err_type) = self.extract_result_types(&result_ty, span)?;

        // 4. Elaborate the body with the Result return context.
        //    `?` inside the body will see Result<T, E> as the return type
        //    and desugar to `return Err(e)` — which exits this scope.
        //    The body itself should evaluate to T (the success value).
        let body_term =
            self.with_return_context(Some(result_ty.clone()), |elab| elab.check(body, &ok_type))?;

        // 5. Wrap in Ok: the body evaluates to T, wrap it as Ok(T) → Result<T, E>
        let ok_term = self.wrap_in_ok(body_term, &result_ty, span)?;

        Ok((ok_term, result_ty))
    }

    /// Elaborate a try block in infer mode (no expected type).
    fn elab_try_block_infer(&mut self, body: &Expr, span: Span) -> ElabResult<(Term, Type)> {
        // Look up Result type to construct Result<T, E> where T comes from body inference.
        // First, check that Result exists in scope
        let has_result = self.env.lookup_constructor("Ok").is_some()
            && self.env.lookup_constructor("Err").is_some();

        if !has_result {
            return Err(ElabError::new(
                span,
                ElabErrorKind::TryBlockRequiresResultType,
            ));
        }

        // Infer the body with a temporary Result return context.
        // We need to know the Result type to set the context, but we don't know E yet.
        // Strategy: infer the body first to get T, then check if `?` usage determined E.
        //
        // For infer mode, we fall back to elaborating body, then wrapping.
        // The `?` inside will fail with TryReturnMismatch if the return context
        // doesn't match — but we set it to None, so `?` will use TryOutsideReturnContext.
        //
        // This means try blocks in infer mode need a type annotation.
        Err(ElabError::cannot_infer(span)
            .with_help("add type annotation: `let result: Result<T, E> = try { ... }`"))
    }

    /// Extract T and E from a Result<T, E> encoding.
    fn extract_result_types(&self, result_ty: &Type, span: Span) -> ElabResult<(Type, Type)> {
        // Look up Ok constructor to find its index → determines which side of Sum is T
        let ok_info = self.env.lookup_constructor("Ok").ok_or_else(|| {
            ElabError::new(
                span,
                ElabErrorKind::TryBlockMissingConstructor("Ok".to_string()),
            )
        })?;
        let err_info = self.env.lookup_constructor("Err").ok_or_else(|| {
            ElabError::new(
                span,
                ElabErrorKind::TryBlockMissingConstructor("Err".to_string()),
            )
        })?;

        // Unfold Mu if needed (Result is non-recursive, but be safe)
        let unfolded = match result_ty {
            Type::Mu(var, body) => body.substitute(var, result_ty),
            other => other.clone(),
        };

        match &unfolded {
            Type::Sum(left, right) => {
                // ok_info.index tells us which side of Sum holds T
                let ok_ty = if ok_info.index == 0 {
                    (**left).clone()
                } else {
                    (**right).clone()
                };
                let err_ty = if err_info.index == 0 {
                    (**left).clone()
                } else {
                    (**right).clone()
                };
                Ok((ok_ty, err_ty))
            }
            _ => Err(ElabError::new(
                span,
                ElabErrorKind::TryBlockExpectedSumEncoding,
            )),
        }
    }

    /// Wrap a term in the Ok constructor for the given Result type.
    ///
    /// Sum encoding order: `type Result<T, E> = Ok(T) | Err(E)` encodes as
    /// `Sum(Err_type, Ok_type)` — constructors are sorted alphabetically,
    /// so Err=Inl (index 0) and Ok=Inr (index 1). We look up the index
    /// dynamically rather than hardcoding, so this works for any Result-like
    /// ADT regardless of constructor ordering.
    fn wrap_in_ok(&self, term: Term, result_ty: &Type, span: Span) -> ElabResult<Term> {
        // Look up Ok constructor index to determine Inl vs Inr
        let ok_info = self.env.lookup_constructor("Ok").ok_or_else(|| {
            ElabError::new(
                span,
                ElabErrorKind::TryBlockMissingConstructor("Ok".to_string()),
            )
        })?;
        if ok_info.index == 0 {
            Ok(Term::inl(result_ty.clone(), term))
        } else {
            Ok(Term::inr(result_ty.clone(), term))
        }
    }
}

/// Scan an expression for `Return` nodes (not descending into lambdas/closures).
/// Returns the span of the first `return` found, if any.
fn find_return_in_expr(expr: &Expr) -> Option<Span> {
    use crate::span::Spanned;
    match expr {
        Expr::Return(_, span) => Some(*span),

        // Don't descend into lambdas — return inside a lambda is fine
        Expr::Lambda(_, _, _) => None,

        // Recurse into sub-expressions
        Expr::Block(stmts, final_expr, _) => stmts
            .iter()
            .find_map(find_return_in_stmt)
            .or_else(|| final_expr.as_ref().and_then(|e| find_return_in_expr(e))),
        Expr::Let(_, _, val, body, _) => {
            find_return_in_expr(val).or_else(|| find_return_in_expr(body))
        }
        Expr::LetElse(_, _, val, else_e, body, _) => find_return_in_expr(val)
            .or_else(|| find_return_in_expr(else_e))
            .or_else(|| find_return_in_expr(body)),
        Expr::If(cond, then_b, else_b, _) => find_return_in_expr(cond)
            .or_else(|| find_return_in_expr(then_b))
            .or_else(|| find_return_in_expr(else_b)),
        Expr::IfLet(_, init, body, else_b, _) => find_return_in_expr(init)
            .or_else(|| find_return_in_expr(body))
            .or_else(|| else_b.as_ref().and_then(|e| find_return_in_expr(e))),
        Expr::IfLetChain(conds, body, else_b, _) => conds
            .iter()
            .find_map(|cond| match cond {
                crate::ast::IfLetCondition::Bind(_, e) | crate::ast::IfLetCondition::Guard(e) => {
                    find_return_in_expr(e)
                }
            })
            .or_else(|| find_return_in_expr(body))
            .or_else(|| else_b.as_ref().and_then(|e| find_return_in_expr(e))),
        Expr::Match(scrut, arms, _) => find_return_in_expr(scrut)
            .or_else(|| arms.iter().find_map(|arm| find_return_in_expr(&arm.body))),
        Expr::App(f, args, _) => {
            find_return_in_expr(f).or_else(|| args.iter().find_map(find_return_in_expr))
        }
        Expr::Binary(l, _, r, _) => find_return_in_expr(l).or_else(|| find_return_in_expr(r)),
        Expr::Unary(_, e, _)
        | Expr::Paren(e, _)
        | Expr::Try(e, _)
        | Expr::Annot(e, _, _)
        | Expr::Field(e, _, _)
        | Expr::TypeApp(e, _, _) => find_return_in_expr(e),
        Expr::TryBlock(_, _) => {
            // Nested try blocks: don't look for return in nested try blocks
            // (they have their own scope)
            None
        }
        Expr::Tuple(elems, _) => elems.iter().find_map(find_return_in_expr),
        Expr::Have(_, _, proof, body, _) => {
            find_return_in_expr(proof).or_else(|| find_return_in_expr(body))
        }
        Expr::Subst(proof, motive, witness, _) => find_return_in_expr(proof)
            .or_else(|| match motive {
                Motive::Expr(e) => find_return_in_expr(e),
                Motive::Lambda(_, _, _) => None,
            })
            .or_else(|| find_return_in_expr(witness)),
        Expr::Sym(proof, _) => find_return_in_expr(proof),
        Expr::Trans(h1, h2, _) => find_return_in_expr(h1).or_else(|| find_return_in_expr(h2)),
        Expr::Cong(f, proof, _) => find_return_in_expr(f).or_else(|| find_return_in_expr(proof)),
        Expr::NatInd(motive, base, step, n, _) => {
            let m = match motive {
                Motive::Expr(e) => find_return_in_expr(e),
                Motive::Lambda(_, _, _) => None,
            };
            m.or_else(|| find_return_in_expr(base))
                .or_else(|| find_return_in_expr(step))
                .or_else(|| find_return_in_expr(n))
        }
        Expr::NatRec(_, base, step, n, _) => find_return_in_expr(base)
            .or_else(|| find_return_in_expr(step))
            .or_else(|| find_return_in_expr(n)),
        Expr::Show(_, e, _) | Expr::Assume(_, _, e, _) => find_return_in_expr(e),
        Expr::RecordLit { spread, fields, .. } => spread
            .as_ref()
            .and_then(|e| find_return_in_expr(e))
            .or_else(|| fields.iter().find_map(|(_, e)| find_return_in_expr(e))),
        Expr::NamedRecord { spread, fields, .. } => spread
            .as_ref()
            .and_then(|e| find_return_in_expr(e))
            .or_else(|| fields.iter().find_map(|(_, e)| find_return_in_expr(e))),
        // Leaves
        Expr::Path(_)
        | Expr::IntLiteral(_, _)
        | Expr::BoolLiteral(_, _)
        | Expr::StringLiteral(_, _)
        | Expr::Unit(_)
        | Expr::Refl(_)
        | Expr::Sorry(_)
        | Expr::Error(_) => None,
    }
}

fn find_return_in_stmt(stmt: &crate::ast::Stmt) -> Option<Span> {
    match stmt {
        crate::ast::Stmt::Expr(e, _) => find_return_in_expr(e),
        crate::ast::Stmt::Let(_, _, val, _) => find_return_in_expr(val),
        crate::ast::Stmt::LetElse(_, _, val, else_e, _) => {
            find_return_in_expr(val).or_else(|| find_return_in_expr(else_e))
        }
        crate::ast::Stmt::Item(_) => None,
    }
}
