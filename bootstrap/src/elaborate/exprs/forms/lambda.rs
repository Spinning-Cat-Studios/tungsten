//! Lambda elaboration.
//!
//! Handles:
//! - `check_lambda` - checking lambdas against expected function types
//! - `infer_lambda` - inferring types of lambdas (all params must be annotated)
//! - `nat_literal` - building natural numbers (small: unary, large: NatLit)

use crate::ast::{Expr, LambdaParam};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Build a natural number literal.
    ///
    /// Uses unary encoding (Zero/Succ) for small numbers (≤ 1000) to maintain
    /// proof compatibility, and NatLit for larger numbers to avoid stack overflow.
    pub(in crate::elaborate::exprs) fn nat_literal(&self, n: u64) -> Term {
        Term::nat_smart(n)
    }

    /// Check a lambda against an expected function type.
    pub(in crate::elaborate::exprs) fn check_lambda(
        &mut self,
        params: &[LambdaParam],
        body: &Expr,
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        // Handle multi-parameter lambdas by currying
        if params.is_empty() {
            // No parameters: check body directly
            return self.check(body, expected);
        }

        // Get the expected parameter type
        let Type::Arrow(param_ty, result_ty) = expected else {
            return Err(ElabError::new(
                span,
                ElabErrorKind::ExpectedType {
                    expected: "function type".to_string(),
                    found: expected.clone(),
                },
            ));
        };

        let param = &params[0];
        let param_name = self.pattern_to_name(&param.pattern)?;

        // If annotation provided, check it matches
        if let Some(ref ann) = param.ty {
            let ann_ty = self.elab_type(ann)?;
            if !self.types_equal(&ann_ty, param_ty) {
                return Err(ElabError::type_mismatch(
                    ann.span(),
                    (**param_ty).clone(),
                    ann_ty,
                ));
            }
        }

        // Bind parameter and elaborate body
        self.env.push_scope();
        self.env
            .bind_local(param_name.clone(), (**param_ty).clone(), self.depth);
        self.depth += 1;

        let body_term = if params.len() == 1 {
            // Install closure return context (ADR 13.5.26d §2.3.1)
            self.with_return_context(Some((**result_ty).clone()), |elab| {
                elab.check(body, result_ty)
            })?
        } else {
            // More parameters: recursively build nested lambda
            self.check_lambda(&params[1..], body, result_ty, span)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        Ok(Term::lambda(param_name, (**param_ty).clone(), body_term))
    }

    /// Infer type of a lambda (all params must have annotations).
    pub(in crate::elaborate::exprs) fn infer_lambda(
        &mut self,
        params: &[LambdaParam],
        body: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if params.is_empty() {
            return self.infer(body);
        }

        let param = &params[0];
        let param_name = self.pattern_to_name(&param.pattern)?;

        // Must have annotation to infer
        let param_ty = match &param.ty {
            Some(ann) => self.elab_type(ann)?,
            None => {
                return Err(ElabError::cannot_infer(param.span)
                    .with_help("add type annotation to lambda parameter"));
            }
        };

        // Bind and elaborate body
        self.env.push_scope();
        self.env
            .bind_local(param_name.clone(), param_ty.clone(), self.depth);
        self.depth += 1;

        let (body_term, body_ty) = if params.len() == 1 {
            // Clear enclosing return context for inferred closures (ADR 13.5.26d §2.3.1)
            self.with_return_context(None, |elab| elab.infer(body))?
        } else {
            self.infer_lambda(&params[1..], body, span)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        let term = Term::lambda(param_name, param_ty.clone(), body_term);
        let ty = Type::arrow(param_ty, body_ty);

        Ok((term, ty))
    }
}
