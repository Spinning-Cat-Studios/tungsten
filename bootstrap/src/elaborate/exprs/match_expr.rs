//! Match expression elaboration entry point.
//!
//! Handles:
//! - `elab_match` - main match elaboration dispatcher
//! - `elab_bool_match` - Bool pattern matching desugared to if-then-else
//! - `elab_simple_match_arm` - simple variable pattern arms
//! - `infer_arm_type` - type inference for match arms

use crate::ast::{self, LiteralPattern, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind, ExpectedContext};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate match expression.
    pub(super) fn elab_match(
        &mut self,
        scrutinee: &ast::Expr,
        arms: &[ast::MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Infer scrutinee type
        let (scrutinee_term, scrutinee_ty) = self.infer(scrutinee)?;

        if arms.is_empty() {
            return Err(ElabError::new(span, ElabErrorKind::NonExhaustiveMatch));
        }

        // Special case: Bool matching - desugar to if-then-else
        if matches!(scrutinee_ty, Type::Bool) {
            return self.elab_bool_match(scrutinee_term, arms, expected, span);
        }

        // Check if we're matching on an ADT (via constructor patterns)
        // We check if ANY arm has a constructor pattern, since catch-all arms might come first
        let has_constructor_pattern = arms
            .iter()
            .any(|arm| matches!(&arm.pattern, Pattern::Constructor(_, _, _)));

        if has_constructor_pattern {
            // Constructor pattern - this is an ADT match
            return self.elab_adt_match(scrutinee_term, scrutinee_ty, arms, expected, span);
        }

        // Fall back to simple sum type matching for variable patterns
        let Type::Sum(left_ty, right_ty) = scrutinee_ty.clone() else {
            return Err(ElabError::unsupported(span, "match on non-sum types")
                .with_help("use constructor patterns for ADT matching"));
        };

        if arms.len() != 2 {
            return Err(ElabError::unsupported(
                span,
                "match with != 2 arms on sum type",
            ));
        }

        // Elaborate both arms
        // For the second arm, add context pointing to the first arm
        let (left_var, left_body) = self.elab_simple_match_arm(&arms[0], &left_ty, expected)?;

        // If no expected type, use the first arm's type as expected for the second arm
        // and add context so errors point to the first arm
        let result_ty = if let Some(expected) = expected {
            expected.clone()
        } else {
            self.infer_arm_type(&arms[0], &left_ty)?
        };

        // Push context pointing to first arm body for better error messages
        self.push_context(ExpectedContext::branch_unification(arms[0].body.span()));
        let (right_var, right_body) =
            self.elab_simple_match_arm(&arms[1], &right_ty, Some(&result_ty))?;
        self.pop_context();

        let term = Term::case(scrutinee_term, left_var, left_body, right_var, right_body);
        Ok((term, result_ty))
    }

    /// Elaborate a simple match arm (variable pattern only).
    pub(super) fn elab_simple_match_arm(
        &mut self,
        arm: &ast::MatchArm,
        scrutinee_ty: &Type,
        expected: Option<&Type>,
    ) -> ElabResult<(String, Term)> {
        // For now, only support variable patterns
        let var_name = self.pattern_to_name(&arm.pattern)?;

        // Guards not supported in Phase 1
        if arm.guard.is_some() {
            return Err(ElabError::unsupported(arm.span, "match guards"));
        }

        // Bind variable and elaborate body
        self.env.push_scope();
        self.env
            .bind_local(var_name.clone(), scrutinee_ty.clone(), self.depth);
        self.depth += 1;

        let body_term = if let Some(expected) = expected {
            self.check(&arm.body, expected)?
        } else {
            self.infer(&arm.body)?.0
        };

        self.depth -= 1;
        self.env.pop_scope();

        Ok((var_name, body_term))
    }

    /// Infer the type of a match arm (without elaborating).
    pub(super) fn infer_arm_type(
        &mut self,
        arm: &ast::MatchArm,
        scrutinee_ty: &Type,
    ) -> ElabResult<Type> {
        let var_name = self.pattern_to_name(&arm.pattern)?;

        self.env.push_scope();
        self.env
            .bind_local(var_name, scrutinee_ty.clone(), self.depth);
        self.depth += 1;

        let (_, ty) = self.infer(&arm.body)?;

        self.depth -= 1;
        self.env.pop_scope();

        Ok(ty)
    }

    /// Elaborate a match on Bool, desugaring to if-then-else.
    ///
    /// Transforms:
    ///   match b { true => e1, false => e2 }
    /// Into:
    ///   if b { e1 } else { e2 }
    fn elab_bool_match(
        &mut self,
        scrutinee: Term,
        arms: &[ast::MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Require exactly 2 arms
        if arms.len() != 2 {
            return Err(ElabError::new(span, ElabErrorKind::NonExhaustiveMatch)
                .with_help("Bool match requires exactly two arms: `true` and `false`"));
        }

        // Check for guards (not supported)
        for arm in arms {
            if arm.guard.is_some() {
                return Err(ElabError::unsupported(arm.span, "match guards"));
            }
        }

        // Extract true/false arms - order doesn't matter
        let (true_arm, false_arm) = self.extract_bool_arms(arms, span)?;

        // Elaborate both arms
        // First, determine expected type from first arm if not provided
        let result_ty = if let Some(expected) = expected {
            expected.clone()
        } else {
            self.infer(&true_arm.body)?.1
        };

        // Elaborate true branch
        let true_term = self.check(&true_arm.body, &result_ty)?;

        // Elaborate false branch with context for error messages
        self.push_context(ExpectedContext::branch_unification(true_arm.body.span()));
        let false_term = self.check(&false_arm.body, &result_ty)?;
        self.pop_context();

        // Build if-then-else term
        let term = Term::if_then_else(scrutinee, true_term, false_term);
        Ok((term, result_ty))
    }

    /// Extract true and false arms from a Bool match.
    /// Returns (true_arm, false_arm) regardless of their order in the source.
    fn extract_bool_arms<'b>(
        &self,
        arms: &'b [ast::MatchArm],
        span: Span,
    ) -> ElabResult<(&'b ast::MatchArm, &'b ast::MatchArm)> {
        let mut true_arm: Option<&ast::MatchArm> = None;
        let mut false_arm: Option<&ast::MatchArm> = None;

        for arm in arms {
            match &arm.pattern {
                Pattern::Literal(LiteralPattern::Bool(true, _)) => {
                    if true_arm.is_some() {
                        return Err(ElabError::new(arm.span, ElabErrorKind::UnreachableArm)
                            .with_help("duplicate `true` pattern"));
                    }
                    true_arm = Some(arm);
                }
                Pattern::Literal(LiteralPattern::Bool(false, _)) => {
                    if false_arm.is_some() {
                        return Err(ElabError::new(arm.span, ElabErrorKind::UnreachableArm)
                            .with_help("duplicate `false` pattern"));
                    }
                    false_arm = Some(arm);
                }
                _ => {
                    return Err(ElabError::new(
                        arm.pattern.span(),
                        ElabErrorKind::UnsupportedPattern(
                            "non-literal pattern in Bool match".to_string(),
                        ),
                    )
                    .with_help("use `true` or `false` literal patterns"));
                }
            }
        }

        match (true_arm, false_arm) {
            (Some(t), Some(f)) => Ok((t, f)),
            (None, _) => Err(ElabError::new(span, ElabErrorKind::NonExhaustiveMatch)
                .with_help("missing `true` pattern")),
            (_, None) => Err(ElabError::new(span, ElabErrorKind::NonExhaustiveMatch)
                .with_help("missing `false` pattern")),
        }
    }
}
