//! `if`, type ascription and `return` — the control-flow arms of `check` /
//! `infer` that need no expected-type machinery of their own.
//!
//! Moved out of `check_infer/mod.rs` when ADR 14.9.26c's numeric arms took
//! the dispatcher past the file-size cap; the seam is "one arm's helper", the
//! same one `natind.rs` and `refl.rs` already sit on.

use crate::ast::Expr;
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

impl Elaborator<'_> {
    /// Infer the type of an if-then-else expression.
    pub(super) fn infer_if(
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
    pub(super) fn check_if(
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
    pub(super) fn infer_annot(
        &mut self,
        inner: &Expr,
        ty: &crate::ast::TypeExpr,
    ) -> ElabResult<(Term, Type)> {
        let expected = self.elab_type(ty)?;
        let term = self.check(inner, &expected)?;
        Ok((term, expected))
    }

    /// Elaborate a `return` expression (ADR 13.5.26d).
    ///
    /// - `return e` checks `e` against the current function's return type
    /// - bare `return` is `return ()`, valid only when return type is Unit
    /// - Type of `return e` is ⊥ (Void)
    pub(super) fn elab_return(
        &mut self,
        inner: Option<&Expr>,
        span: crate::span::Span,
    ) -> ElabResult<(Term, Type)> {
        let ret_ty = match &self.current_return_type {
            Some(ty) => ty.clone(),
            None => {
                return Err(ElabError::new(span, ElabErrorKind::ReturnOutsideFunction));
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
