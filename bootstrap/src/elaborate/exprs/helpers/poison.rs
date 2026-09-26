//! Poison transit at the construction boundaries (ADR 15.8.26d).
//!
//! ADR 14.8.26g's producers leave a failed type as a stub whose encoding is
//! `Type::Error` (`TypeDef::is_poison`). Its compare and application
//! boundaries already pass that poison through; the sites that BUILD or TAKE
//! APART a value of the type — a record literal, a constructor call, a match
//! — did not, and turned one fault into one complaint per use site (V3 = 15,
//! V5 = 24 on the seeded corpus). These are the transit arms they share.
//!
//! Two rules, both from the ADR's Non-Goals:
//!
//! * the expressions the site would have checked against the type's fields
//!   are still elaborated, so THEIR faults surface — only the complaint about
//!   the poisoned type itself is folded;
//! * the result is a hole (`Term::Sorry` at `Type::Error`), never a
//!   half-built term: the run is refused at codegen once any error is
//!   recorded (14.8.26g D8), so no term built here is ever lowered.

use crate::ast::{Expr, MatchArm, Pattern};
use crate::elaborate::error::ExpectedContext;
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use super::PatternBinding;

impl<'a> Elaborator<'a> {
    /// Transit for a value built from a poisoned type: elaborate each
    /// argument against `Type::Error` (poison unifies with anything, so only
    /// the argument's own faults can surface) and return a hole.
    pub(in crate::elaborate::exprs) fn elab_poisoned_construction<'e>(
        &mut self,
        args: impl IntoIterator<Item = &'e Expr>,
    ) -> ElabResult<Term> {
        for arg in args {
            self.check(arg, &Type::Error)?;
        }
        Ok(Term::Sorry)
    }

    /// Transit for a match whose scrutinee's type already failed: every
    /// variable a pattern introduces is bound to `Type::Error`, each guard
    /// and body is elaborated (against `expected`, else unified with the
    /// first arm's inferred type, as the healthy path does), and the match
    /// itself is a hole.
    pub(in crate::elaborate::exprs) fn elab_poisoned_match(
        &mut self,
        arms: &[MatchArm],
        expected: Option<&Type>,
    ) -> ElabResult<(Term, Type)> {
        let mut result_ty = expected.cloned();
        for (index, arm) in arms.iter().enumerate() {
            let mut bindings = Vec::new();
            collect_poisoned_pattern_bindings(&arm.pattern, &mut bindings);
            // Without an expected type the later arms unify with the first,
            // and a mismatch there points at the first arm's body — the
            // healthy path's convention.
            let unifies_with_first_arm = expected.is_none() && index != 0;
            if unifies_with_first_arm {
                self.push_context(ExpectedContext::branch_unification(arms[0].body.span()));
            }
            let arm_ty = self.with_pattern_bindings(&bindings, |elab| {
                if let Some(guard) = &arm.guard {
                    elab.check(guard, &Type::Bool)?;
                }
                match &result_ty {
                    Some(ty) => elab.check(&arm.body, ty).map(|_| ty.clone()),
                    None => elab.infer(&arm.body).map(|(_, ty)| ty),
                }
            });
            if unifies_with_first_arm {
                self.pop_context();
            }
            result_ty.get_or_insert(arm_ty?);
        }
        Ok((Term::Sorry, result_ty.unwrap_or(Type::Error)))
    }

    /// Run `f` with every binding in scope, restoring the scope and depth
    /// afterwards.
    pub(in crate::elaborate::exprs) fn with_pattern_bindings<T>(
        &mut self,
        bindings: &[PatternBinding],
        f: impl FnOnce(&mut Self) -> ElabResult<T>,
    ) -> ElabResult<T> {
        self.env.push_scope();
        for binding in bindings {
            self.env
                .bind_local(binding.var_name.clone(), binding.var_ty.clone(), self.depth);
            self.depth += 1;
        }
        let result = f(self);
        self.depth -= bindings.len();
        self.env.pop_scope();
        result
    }
}

/// Every variable `pattern` introduces, bound to `Type::Error` — the
/// binding a pattern against a poisoned value gets, whatever its nesting.
///
/// A pure walk over the AST: a `Var` binds, the combining forms recurse, and
/// the rest (`_`, literals, the parser's error placeholder) bind nothing.
pub(in crate::elaborate::exprs) fn collect_poisoned_pattern_bindings(
    pattern: &Pattern,
    bindings: &mut Vec<PatternBinding>,
) {
    match pattern {
        Pattern::Var(ident) => bindings.push(PatternBinding {
            var_name: ident.name.clone(),
            var_ty: Type::Error,
        }),
        Pattern::Tuple(subs, _) | Pattern::Constructor(_, subs, _) => {
            for sub in subs {
                collect_poisoned_pattern_bindings(sub, bindings);
            }
        }
        Pattern::Or(left, right, _) => {
            collect_poisoned_pattern_bindings(left, bindings);
            collect_poisoned_pattern_bindings(right, bindings);
        }
        Pattern::Wildcard(_) | Pattern::Literal(_) | Pattern::Error(_) => {}
    }
}
