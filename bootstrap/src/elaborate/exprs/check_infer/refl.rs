//! `refl` checking against equality types (ADR 21.5.26d).

use tungsten_core::{Term, Type};

use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl Elaborator<'_> {
    /// Check `refl` against an expected type.
    ///
    /// Succeeds only when `expected` is `Eq(τ, t1, t2)` and `t1` and `t2` are
    /// definitionally equal (normalize to the same term).
    pub(crate) fn check_refl(
        &mut self,
        span: crate::span::Span,
        expected: &Type,
    ) -> ElabResult<Term> {
        match expected {
            Type::Eq(ty, t1, t2) => {
                if self.terms_definitionally_equal(t1, t2, ty) {
                    Ok(Term::refl((**ty).clone(), (**t1).clone()))
                } else {
                    Err(ElabError::invalid_refl(
                        span,
                        (**t1).clone(),
                        (**t2).clone(),
                    ))
                }
            }
            _ => Err(ElabError::refl_expected_equality(span, expected.clone())),
        }
    }
}
