//! `subst` elaboration (ADR 21.5.26d, 21.5.26g).
//!
//! `subst(proof, motive, witness)` implements equality transport:
//! given `proof : Eq(τ, a, b)`, a motive `P : τ → Type`, and
//! `witness : P(a)`, produces a term of type `P(b)`.
//!
//! The motive must be a typed predicate lambda `|x: τ| <type-body>`.
//! The body is elaborated as a type expression with `x : τ` in scope.
//! Non-lambda motives are rejected with `MotiveNotPredicate`.

use tungsten_core::{Term, Type};

use crate::ast::{Expr, Motive};
use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::{Span, Spanned};

impl Elaborator<'_> {
    /// Elaborate a motive lambda `|x: τ| <type-body>` and return the elaborated body type.
    ///
    /// Binds `x : τ` as a local variable, then elaborates the body as a type expression.
    /// Returns the elaborated body type (with `x` in scope).
    fn elab_motive(&mut self, motive: &Motive, base_ty: &Type, span: Span) -> ElabResult<Type> {
        match motive {
            Motive::Lambda(param, body, _motive_span) => {
                // Extract parameter name
                let param_name = self.pattern_to_name(&param.pattern)?;

                // Validate parameter type annotation exists and matches base type
                let param_ty = match &param.ty {
                    Some(ann) => {
                        let ann_ty = self.elab_type(ann)?;
                        if !self.types_equal(&ann_ty, base_ty) {
                            return Err(ElabError::motive_domain_mismatch(
                                ann.span(),
                                base_ty.clone(),
                                ann_ty,
                            ));
                        }
                        ann_ty
                    }
                    // Motive parameter must have a type annotation (ADR 21.5.26g)
                    None => {
                        return Err(ElabError::motive_not_predicate(param.span, Type::Unit));
                    }
                };

                // Bind parameter and elaborate body as type expression
                self.env.push_scope();
                self.env.bind_local(param_name, param_ty, self.depth);
                self.depth += 1;

                let body_ty = self
                    .elab_type(body)
                    .map_err(|_| ElabError::motive_body_not_type(body.span()));

                self.depth -= 1;
                self.env.pop_scope();

                body_ty
            }
            Motive::Expr(expr) => {
                // Non-lambda motives are rejected
                let (_, expr_ty) = self.infer(expr).unwrap_or((Term::Sorry, Type::Unit));
                Err(ElabError::motive_not_predicate(expr.span(), expr_ty))
            }
        }
    }

    /// Infer the type of `subst(proof, motive, witness)`.
    ///
    /// 1. Infer `proof` → must be `Eq(τ, a, b)`
    /// 2. Elaborate motive `|x: τ| P(x)` → validates predicate form
    /// 3. Infer `witness` → witness_type used as result type (for infer mode)
    pub(crate) fn infer_subst(
        &mut self,
        proof_expr: &Expr,
        motive: &Motive,
        witness_expr: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Infer proof type — must be Eq(τ, a, b)
        let (proof_term, proof_ty) = self.infer(proof_expr)?;
        let base_ty = match &proof_ty {
            Type::Eq(ty, _, _) => (**ty).clone(),
            _ => return Err(ElabError::subst_expected_equality(span, proof_ty)),
        };

        // 2. Elaborate motive — validates predicate form
        let motive_body_ty = self.elab_motive(motive, &base_ty, span)?;

        // 3. Infer witness type (used as result type in infer mode)
        let (witness_term, witness_ty) = self.infer(witness_expr)?;

        let motive_type = Type::Arrow(Box::new(base_ty.clone()), Box::new(motive_body_ty));
        let term = Term::subst(base_ty, motive_type, proof_term, witness_term);

        Ok((term, witness_ty))
    }

    /// Check `subst(proof, motive, witness)` against an expected type.
    ///
    /// 1. Infer `proof` → must be `Eq(τ, a, b)`
    /// 2. Elaborate motive `|x: τ| P(x)` → validates predicate form
    /// 3. Check `witness` against the expected type
    /// 4. Result type = expected type
    pub(crate) fn check_subst(
        &mut self,
        proof_expr: &Expr,
        motive: &Motive,
        witness_expr: &Expr,
        expected: &Type,
        span: Span,
    ) -> ElabResult<Term> {
        // 1. Infer proof type — must be Eq(τ, a, b)
        let (proof_term, proof_ty) = self.infer(proof_expr)?;
        let base_ty = match &proof_ty {
            Type::Eq(ty, _, _) => (**ty).clone(),
            _ => return Err(ElabError::subst_expected_equality(span, proof_ty)),
        };

        // 2. Elaborate motive — validates predicate form
        let motive_body_ty = self.elab_motive(motive, &base_ty, span)?;

        // 3. Check witness against expected type
        let witness_term = self.check(witness_expr, expected)?;

        // 4. Store motive as Arrow(τ, body_ty) and produce Term::Subst
        let motive_type = Type::Arrow(Box::new(base_ty.clone()), Box::new(motive_body_ty));
        Ok(Term::subst(base_ty, motive_type, proof_term, witness_term))
    }
}
