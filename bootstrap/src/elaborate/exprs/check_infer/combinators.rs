//! Derived equality combinators (ADR 21.5.26d, Phase 3).
//!
//! `sym`, `trans`, `cong` are elaborator desugaring into `Term::Subst` — no new
//! core calculus primitives. Each combinator infers its arguments' types and
//! constructs the appropriate `Term::Subst` with the computed result type.

use tungsten_core::{Term, Type};

use crate::ast::Expr;
use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;

impl Elaborator<'_> {
    /// Validate a compiler-synthesised motive type (ADR 21.5.26g, P4).
    ///
    /// Derived combinators (`sym`, `trans`, `cong`) construct motives as
    /// `Arrow(base_ty, Prop)`. This function validates the synthesised motive
    /// through the same domain check as user-written motives — no trusted bypass.
    fn validate_synthesised_motive(
        &self,
        motive: &Type,
        base_ty: &Type,
        span: Span,
    ) -> ElabResult<()> {
        match motive {
            Type::Arrow(domain, _) => {
                if !self.types_equal(domain, base_ty) {
                    return Err(ElabError::motive_domain_mismatch(
                        span,
                        base_ty.clone(),
                        (**domain).clone(),
                    ));
                }
                Ok(())
            }
            _ => Err(ElabError::motive_not_predicate(span, motive.clone())),
        }
    }

    /// `sym(h)` where `h : Eq(τ, a, b)` → `Eq(τ, b, a)`
    ///
    /// Desugars to: `subst(h, |x| Eq(τ, x, a), refl(τ, a))`
    pub(crate) fn infer_sym(&mut self, proof_expr: &Expr, span: Span) -> ElabResult<(Term, Type)> {
        let (proof_term, proof_ty) = self.infer(proof_expr)?;
        let (base_ty, a, b) = extract_eq(&proof_ty, span)?;

        // Motive: |x| Eq(τ, x, a) — stored as Arrow(τ, Prop) placeholder
        let motive = Type::Arrow(Box::new(base_ty.clone()), Box::new(Type::Prop));
        self.validate_synthesised_motive(&motive, &base_ty, span)?;
        // Witness: refl(τ, a) proves Eq(τ, a, a) = motive(a)
        let witness = Term::refl(base_ty.clone(), a.clone());
        // Result type: Eq(τ, b, a) = motive(b)
        let result_ty = Type::eq(base_ty.clone(), b.clone(), a.clone());

        let term = Term::subst(base_ty, motive, proof_term, witness);
        Ok((term, result_ty))
    }

    /// `trans(h1, h2)` where `h1 : Eq(τ, a, b)`, `h2 : Eq(τ, b, c)` → `Eq(τ, a, c)`
    ///
    /// Desugars to: `subst(h2, |x| Eq(τ, a, x), h1)`
    pub(crate) fn infer_trans(
        &mut self,
        h1_expr: &Expr,
        h2_expr: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        let (h1_term, h1_ty) = self.infer(h1_expr)?;
        let (base_ty, a, b) = extract_eq(&h1_ty, span)?;

        let (h2_term, h2_ty) = self.infer(h2_expr)?;
        let (_, b2, c) = extract_eq(&h2_ty, span)?;

        // Check that h1's right endpoint matches h2's left endpoint
        if !self.terms_definitionally_equal(&b, &b2, &base_ty) {
            return Err(ElabError::trans_endpoint_mismatch(span, b, b2));
        }

        // Motive: |x| Eq(τ, a, x) — stored as Arrow(τ, Prop) placeholder
        let motive = Type::Arrow(Box::new(base_ty.clone()), Box::new(Type::Prop));
        self.validate_synthesised_motive(&motive, &base_ty, span)?;
        // Witness: h1 proves Eq(τ, a, b) = motive(b)
        // Result type: Eq(τ, a, c) = motive(c)
        let result_ty = Type::eq(base_ty.clone(), a.clone(), c.clone());

        let term = Term::subst(base_ty, motive, h2_term, h1_term);
        Ok((term, result_ty))
    }

    /// `cong(f, h)` where `f : τ → σ`, `h : Eq(τ, a, b)` → `Eq(σ, f(a), f(b))`
    ///
    /// Desugars to: `subst(h, |x| Eq(σ, f(a), f(x)), refl(σ, f(a)))`
    pub(crate) fn infer_cong(
        &mut self,
        f_expr: &Expr,
        proof_expr: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        let (f_term, f_ty) = self.infer(f_expr)?;
        let (proof_term, proof_ty) = self.infer(proof_expr)?;
        let (base_ty, a, b) = extract_eq(&proof_ty, span)?;

        // Extract codomain from f's type
        let codomain = match &f_ty {
            Type::Arrow(_, cod) => (**cod).clone(),
            _ => return Err(ElabError::cong_expected_function(span, f_ty)),
        };

        // Compute f(a) and f(b) as term applications
        let f_a = Term::app(f_term.clone(), a.clone());
        let f_b = Term::app(f_term, b.clone());

        // Motive: |x| Eq(σ, f(a), f(x)) — stored as Arrow(τ, Prop) placeholder
        let motive = Type::Arrow(Box::new(base_ty.clone()), Box::new(Type::Prop));
        self.validate_synthesised_motive(&motive, &base_ty, span)?;
        // Witness: refl(σ, f(a)) proves Eq(σ, f(a), f(a)) = motive(a)
        let witness = Term::refl(codomain.clone(), f_a.clone());
        // Result type: Eq(σ, f(a), f(b)) = motive(b)
        let result_ty = Type::eq(codomain, f_a, f_b);

        let term = Term::subst(base_ty, motive, proof_term, witness);
        Ok((term, result_ty))
    }
}

/// Extract `(τ, a, b)` from an `Eq(τ, a, b)` type, or produce an error.
fn extract_eq(ty: &Type, span: Span) -> ElabResult<(Type, Term, Term)> {
    match ty {
        Type::Eq(base, a, b) => Ok(((**base).clone(), (**a).clone(), (**b).clone())),
        _ => Err(ElabError::subst_expected_equality(span, ty.clone())),
    }
}
