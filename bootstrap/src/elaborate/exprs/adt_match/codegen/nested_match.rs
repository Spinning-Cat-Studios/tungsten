//! Nested pattern matching for ADT codegen.
//!
//! Handles cases where multiple arms match the same outer constructor with
//! different inner patterns, e.g.:
//! ```text
//! Some(TokBang()) => ..., Some(TokMinus()) => ..., _ => ...
//! ```
//! This generates an inner match on the payload type.

use crate::ast::{self, Pattern};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::env as elab_env;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::exprs::adt_match::context::AdtIdentity;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Build a nested match for multiple arms that match the same outer constructor
    /// with different inner patterns.
    ///
    /// For example: `Some(TokBang()) => ..., Some(TokMinus()) => ..., _ => ...`
    /// becomes an inner match on TokenKind for the `Some` constructor's payload.
    ///
    /// If the inner patterns are NOT constructor patterns (e.g., variables),
    /// we just use the first arm since the others are unreachable.
    ///
    /// IMPORTANT: Nested matching only works for single-field constructors where
    /// the payload is a sum/ADT type. For multi-field constructors (product types),
    /// we emit a compile error — silently using only the first arm would drop
    /// subsequent arms and generate `unreachable` IR for valid match branches.
    pub(in crate::elaborate::exprs::adt_match) fn build_nested_match_for_arms(
        &mut self,
        arms: &[&ast::MatchArm],
        catch_all_arm: Option<&ast::MatchArm>,
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
        adt: &AdtIdentity<'_>,
    ) -> ElabResult<(String, Term)> {
        // Check if nested matching is possible and beneficial
        if !self.should_build_nested_match(arms, payload_ty, outer_ctor) {
            // Multi-field constructors with nested patterns are not supported.
            // Emit a compile error rather than silently dropping arms, which
            // would generate `unreachable` for valid match branches (ADR 18.4.26a).
            if arms.len() > 1 && Self::arms_have_nested_constructors(arms) {
                return Err(ElabError::new(
                    arms[0].pattern.span(),
                    ElabErrorKind::Other(format!(
                        "nested constructor patterns in multi-field constructor `{}` are not supported; \
                         rewrite as an explicit inner match on the nested field",
                        outer_ctor.name,
                    )),
                ));
            }
            // Single arm or no nested constructors — safe to use first arm
            return self.elab_ctor_arm_or_catch_all(
                arms[0], payload_ty, outer_ctor, adt,
                None, // nested match fallback — no result_ty threading needed
            );
        }

        // Extract inner patterns from constructor arms
        let inner_arms = self.extract_inner_arms(arms)?;

        // Build the inner match with proper scoping
        self.build_scoped_inner_match(&inner_arms, catch_all_arm, payload_ty, outer_ctor)
    }

    /// Determine if we should build a nested match for these arms.
    ///
    /// Returns true only if:
    /// 1. At least one arm has a nested constructor pattern
    /// 2. The outer constructor has exactly one field
    /// 3. The payload type is a matchable ADT/sum
    fn should_build_nested_match(
        &self,
        arms: &[&ast::MatchArm],
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
    ) -> bool {
        let has_nested_constructors = Self::arms_have_nested_constructors(arms);
        let is_single_field = outer_ctor.fields.len() == 1;
        let payload_is_matchable = self.try_resolve_adt_from_type(payload_ty);

        has_nested_constructors && is_single_field && payload_is_matchable
    }

    /// Check if any arm has nested constructor patterns.
    fn arms_have_nested_constructors(arms: &[&ast::MatchArm]) -> bool {
        arms.iter().any(|arm| {
            if let Pattern::Constructor(_, sub_patterns, _) = &arm.pattern {
                sub_patterns
                    .iter()
                    .any(|p| matches!(p, Pattern::Constructor(..)))
            } else {
                false
            }
        })
    }

    /// Extract inner patterns from constructor arms.
    ///
    /// Each arm's outer constructor pattern is unwrapped to get the inner pattern.
    fn extract_inner_arms(&self, arms: &[&ast::MatchArm]) -> ElabResult<Vec<ast::MatchArm>> {
        let mut inner_arms = Vec::with_capacity(arms.len());

        for arm in arms {
            let inner_arm = self.extract_single_inner_arm(arm)?;
            inner_arms.push(inner_arm);
        }

        Ok(inner_arms)
    }

    /// Extract the inner pattern from a single constructor arm.
    fn extract_single_inner_arm(&self, arm: &ast::MatchArm) -> ElabResult<ast::MatchArm> {
        match &arm.pattern {
            Pattern::Constructor(_, sub_patterns, _) => {
                if sub_patterns.len() != 1 {
                    return Err(ElabError::new(
                        arm.pattern.span(),
                        ElabErrorKind::Other(
                            "nested patterns with multiple fields not yet supported".to_string(),
                        ),
                    ));
                }

                Ok(ast::MatchArm {
                    pattern: sub_patterns[0].clone(),
                    guard: arm.guard.clone(),
                    body: arm.body.clone(),
                    span: arm.span,
                })
            }
            _ => Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::Other("expected constructor pattern for nested match".to_string()),
            )),
        }
    }

    /// Build inner match with proper scope management.
    fn build_scoped_inner_match(
        &mut self,
        inner_arms: &[ast::MatchArm],
        catch_all_arm: Option<&ast::MatchArm>,
        payload_ty: &Type,
        outer_ctor: &elab_env::Constructor,
    ) -> ElabResult<(String, Term)> {
        let raw_var = format!("__nested_{}", outer_ctor.name);

        // Bind the variable and build inner match in new scope
        self.env.push_scope();
        self.env
            .bind_local(raw_var.clone(), payload_ty.clone(), self.depth);
        self.depth += 1;

        let scrutinee = Term::var(&raw_var);
        let inner_body =
            self.elaborate_match_with_arms(&scrutinee, payload_ty, inner_arms, catch_all_arm)?;

        self.depth -= 1;
        self.env.pop_scope();

        Ok((raw_var, inner_body))
    }

    /// Elaborate a match expression with given arms (used for nested pattern handling).
    fn elaborate_match_with_arms(
        &mut self,
        scrutinee: &Term,
        scrutinee_ty: &Type,
        inner_arms: &[ast::MatchArm],
        outer_catch_all: Option<&ast::MatchArm>,
    ) -> ElabResult<Term> {
        // Combine inner_arms with catch_all if present
        let mut all_arms: Vec<ast::MatchArm> = inner_arms.to_vec();
        if let Some(catch_all) = outer_catch_all {
            all_arms.push(catch_all.clone());
        }

        // Check if this is actually an ADT type - if not, fall back to simple match
        // Try to get ADT info - if it fails, we can't do ADT match
        let is_adt = self.try_resolve_adt_from_type(scrutinee_ty);

        if is_adt {
            // Delegate to elab_adt_match which handles all the phases
            let (term, _ty) = self.elab_adt_match(
                scrutinee.clone(),
                scrutinee_ty.clone(),
                &all_arms,
                None, // no expected type
                Span::new(0, 0),
            )?;
            Ok(term)
        } else {
            // Not an ADT - use the first arm's body directly
            // This handles cases like matching on non-sum types
            if let Some(first_arm) = all_arms.first() {
                let (term, _ty) = self.infer(&first_arm.body)?;
                Ok(term)
            } else {
                Err(ElabError::new(
                    Span::new(0, 0),
                    ElabErrorKind::Other("no arms for nested match".to_string()),
                ))
            }
        }
    }

    /// Try to resolve ADT info from a type, returning true if it's an ADT we can match on.
    pub(in crate::elaborate::exprs::adt_match) fn try_resolve_adt_from_type(
        &self,
        ty: &Type,
    ) -> bool {
        match ty {
            Type::Adt(name, _, _) => self.env.lookup_type(name).is_some(),
            Type::App(name, _) => {
                // Cross-module type reference - check if it resolves to an ADT
                self.env.lookup_type(name).is_some()
            }
            Type::Mu(_, body) => self.try_resolve_adt_from_type(body),
            Type::Sum(_, _) => true, // Binary sum is also matchable
            _ => false,
        }
    }
}
