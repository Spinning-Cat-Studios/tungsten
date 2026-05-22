//! `natind` and `natrec` elaboration (ADR 22.5.26a).
//!
//! `natind(motive, base, step, n)` — natural number induction.
//! `natrec(type, base, step, n)` — natural number primitive recursion.

use tungsten_core::{Term, Type};

use crate::ast::{Expr, Motive, TypeExpr};
use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::{Span, Spanned};

impl Elaborator<'_> {
    /// Elaborate `natind(motive, base, step, n)`.
    ///
    /// Motive must be `|k: Nat| <type-expr>`. Base is checked against motive(Zero).
    /// Step is checked against `Nat → motive_body → motive_body` (simplified — the
    /// full dependent step type `∀k. P(k) → P(succ k)` requires δ-reduction).
    /// Result type: motive_body (the motive applied to n).
    pub(crate) fn infer_natind(
        &mut self,
        motive: &Motive,
        base_expr: &Expr,
        step_expr: &Expr,
        n_expr: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Elaborate motive — must be |k: Nat| <type-expr>
        let motive_body_ty = self.elab_natind_motive(motive, span)?;

        // 2. Elaborate base case — check against motive body type
        //    (In a full dependent setting this would be P(Zero), but without
        //     δ-reduction the motive body doesn't reduce on Zero for open motives.
        //     We check against the static motive body type.)
        let base_term = self.check(base_expr, &motive_body_ty)?;

        // 3. Elaborate step case — Nat → motive_body → motive_body
        let step_ty = Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::Arrow(
                Box::new(motive_body_ty.clone()),
                Box::new(motive_body_ty.clone()),
            )),
        );
        let step_term = self.check(step_expr, &step_ty)?;

        // 4. Elaborate target — must be Nat
        let (n_term, n_ty) = self.infer(n_expr)?;
        if !self.types_equal(&n_ty, &Type::Nat) {
            return Err(ElabError::type_mismatch(n_expr.span(), Type::Nat, n_ty));
        }

        // 5. Produce Term::NatInd
        let motive_type = Type::Arrow(Box::new(Type::Nat), Box::new(motive_body_ty.clone()));
        let term = Term::natind(motive_type, base_term, step_term, n_term);

        Ok((term, motive_body_ty))
    }

    /// Elaborate `natrec(type, base, step, n)`.
    ///
    /// Result type T. Base : T. Step : Nat → T → T.
    pub(crate) fn infer_natrec(
        &mut self,
        result_type_expr: &TypeExpr,
        base_expr: &Expr,
        step_expr: &Expr,
        n_expr: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // 1. Elaborate result type
        let result_ty = self.elab_type(result_type_expr)?;

        // 2. Elaborate base case — check against T
        let base_term = self.check(base_expr, &result_ty)?;

        // 3. Elaborate step case — Nat → T → T
        let step_ty = Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::Arrow(
                Box::new(result_ty.clone()),
                Box::new(result_ty.clone()),
            )),
        );
        let step_term = self.check(step_expr, &step_ty)?;

        // 4. Elaborate target — must be Nat
        let (n_term, n_ty) = self.infer(n_expr)?;
        if !self.types_equal(&n_ty, &Type::Nat) {
            return Err(ElabError::type_mismatch(n_expr.span(), Type::Nat, n_ty));
        }

        // 5. Produce Term::NatRec
        let term = Term::natrec(result_ty.clone(), base_term, step_term, n_term);

        Ok((term, result_ty))
    }

    /// Elaborate a motive for `natind`, validating that the domain is `Nat`.
    fn elab_natind_motive(&mut self, motive: &Motive, span: Span) -> ElabResult<Type> {
        match motive {
            Motive::Lambda(param, body, _motive_span) => {
                let param_name = self.pattern_to_name(&param.pattern)?;

                let param_ty = match &param.ty {
                    Some(ann) => {
                        let ann_ty = self.elab_type(ann)?;
                        if !self.types_equal(&ann_ty, &Type::Nat) {
                            return Err(ElabError::natind_motive_not_nat(ann.span(), ann_ty));
                        }
                        ann_ty
                    }
                    None => {
                        return Err(ElabError::motive_not_predicate(param.span, Type::Unit));
                    }
                };

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
                let (_, expr_ty) = self.infer(expr).unwrap_or((Term::Sorry, Type::Unit));
                Err(ElabError::motive_not_predicate(expr.span(), expr_ty))
            }
        }
    }
}
