//! Proof construct elaboration.
//!
//! Handles:
//! - `have h: P = proof; body` - proof binding
//! - `show P { proof }` - type ascription
//! - `assume h: P; body` - lambda introduction

use crate::ast::{self, Expr};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate `have h: P = proof; body`.
    pub(super) fn elab_have(
        &mut self,
        name: &ast::Ident,
        prop: &ast::TypeExpr,
        proof: &Expr,
        body: &Expr,
        expected: Option<&Type>,
        _span: Span,
    ) -> ElabResult<(Term, Type)> {
        // have h: P = proof; body  →  let h: P = proof in body
        let prop_ty = self.elab_type(prop)?;
        let proof_term = self.check(proof, &prop_ty)?;

        self.env.push_scope();
        self.env
            .bind_local(name.name.clone(), prop_ty.clone(), self.depth);
        self.depth += 1;

        let (body_term, body_ty) = if let Some(expected) = expected {
            let term = self.check(body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(body)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        let term = Term::let_in(&name.name, prop_ty, proof_term, body_term);
        Ok((term, body_ty))
    }

    /// Elaborate `show P { proof }`.
    pub(super) fn elab_show(
        &mut self,
        prop: &ast::TypeExpr,
        proof: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // show P { proof }  →  (proof : P)
        let prop_ty = self.elab_type(prop)?;

        // If we have an expected type, verify it matches
        if let Some(expected) = expected {
            if *expected != prop_ty {
                return Err(ElabError::type_mismatch(span, expected.clone(), prop_ty));
            }
        }

        let term = self.check(proof, &prop_ty)?;
        Ok((term, prop_ty))
    }

    /// Elaborate `assume h: P; body`.
    pub(super) fn elab_assume(
        &mut self,
        name: &ast::Ident,
        prop: &ast::TypeExpr,
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // assume h: P; body  →  λ (h: P). body
        let prop_ty = self.elab_type(prop)?;

        // Expected type must be P → result_ty
        let result_expected = if let Some(expected) = expected {
            let Type::Arrow(param_ty, result_ty) = expected else {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::ExpectedType {
                        expected: "function type".to_string(),
                        found: expected.clone(),
                    },
                ));
            };
            if !self.types_equal(&**param_ty, &prop_ty) {
                return Err(ElabError::type_mismatch(
                    prop.span(),
                    (**param_ty).clone(),
                    prop_ty,
                ));
            }
            Some(&**result_ty)
        } else {
            None
        };

        self.env.push_scope();
        self.env
            .bind_local(name.name.clone(), prop_ty.clone(), self.depth);
        self.depth += 1;

        let (body_term, body_ty) = if let Some(expected) = result_expected {
            let term = self.check(body, expected)?;
            (term, expected.clone())
        } else {
            self.infer(body)?
        };

        self.depth -= 1;
        self.env.pop_scope();

        let term = Term::lambda(&name.name, prop_ty.clone(), body_term);
        let ty = Type::arrow(prop_ty, body_ty);
        Ok((term, ty))
    }
}
