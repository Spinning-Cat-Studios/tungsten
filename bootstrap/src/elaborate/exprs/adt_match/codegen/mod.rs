//! Phase 6: Code Generation
//!
//! Generates case expressions for ADT pattern matching.
//!
//! ## Representation Policy (ADR 2.2.26)
//!
//! - n = 1: No case needed, direct binding
//! - n = 2: Binary sum → nested Term::case (existing)
//! - n >= 3: Flat ADT → Term::adt_match with switch dispatch
//!
//! ## Nested Pattern Handling (ADR 3.2.26)
//!
//! When multiple arms match the same outer constructor with different inner patterns:
//! e.g., `Some(TokBang()) => ..., Some(TokMinus()) => ..., _ => ...`
//!
//! The codegen groups these into a single arm for `Some` that contains an inner match
//! on the payload. The catch-all propagates to the inner match.

mod debug;
mod flat_adt;
mod nested_match;

use std::collections::HashMap;

use crate::ast;
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::env as elab_env;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

use super::context::AdtCodegenCtx;
use debug::debug_match_enabled;
use debug::debug_unfold_enabled;

impl<'a> Elaborator<'a> {
    /// Get a single arm for a constructor, handling both single-arm and multi-arm cases.
    ///
    /// When multiple arms match the same outer constructor (e.g., `Some(A)`, `Some(B)`),
    /// this function returns the FIRST arm. The multi-arm case needs special handling
    /// in `build_nested_match_for_arms` which builds an inner match.
    fn get_arm_for_ctor<'b>(
        ctor_arms: &HashMap<usize, Vec<&'b ast::MatchArm>>,
        ctor_index: usize,
        catch_all_arm: Option<&'b ast::MatchArm>,
    ) -> Option<&'b ast::MatchArm> {
        ctor_arms
            .get(&ctor_index)
            .and_then(|arms| arms.first().copied())
            .or(catch_all_arm)
    }

    /// Check if constructor has multiple arms (needs inner match).
    fn has_multiple_arms(
        ctor_arms: &HashMap<usize, Vec<&ast::MatchArm>>,
        ctor_index: usize,
    ) -> bool {
        ctor_arms
            .get(&ctor_index)
            .map(|v| v.len() > 1)
            .unwrap_or(false)
    }

    /// Build nested case analysis for ADT matching.
    ///
    /// For n constructors with right-nested sum: `A + (B + (C + D))`
    /// - `case scrutinee of inl(a) => arm0 | inr(rest) => recurse`
    ///
    /// If `catch_all_arm` is Some, use it for any constructor indices not in `ctor_arms`.
    pub(super) fn build_adt_match(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        ctx: &AdtCodegenCtx,
        ctor_index: usize,
        result_ty: &Type,
    ) -> ElabResult<Term> {
        let num_ctors = ctx.constructors.len();

        // Handle edge cases: single constructor or last constructor
        if num_ctors == 1 {
            return self.build_single_ctor_match(
                scrutinee, sum_type, ctx, None, // single constructor = first arm, always infer
            );
        }

        if ctor_index == num_ctors - 1 {
            return self.build_last_ctor_match(
                scrutinee,
                sum_type,
                ctx,
                ctor_index,
                Some(result_ty), // last ctor is always a subsequent arm
            );
        }

        // Extract left and right types from the sum
        let (left_ty, right_ty) = self.extract_sum_types(sum_type, ctx, ctor_index, num_ctors)?;

        // Debug output at start of match
        self.debug_adt_match_start(
            ctx.ctor_arms,
            ctx.catch_all_arm,
            ctx.constructors,
            ctor_index,
            ctx.adt_name,
        );

        // Elaborate the left arm (current constructor)

        let (left_var, left_body) = self.elaborate_ctor_arm(
            ctx,
            left_ty,
            &ctx.constructors[ctor_index],
            ctor_index,
            Some(result_ty),
        )?;

        // Debug output for specific constructors
        self.debug_ctor_body(&left_var, &left_body, ctor_index, ctx.adt_name);

        // Recursively build the right side
        let (right_var, right_body) =
            self.build_right_branch(right_ty, ctx, ctor_index, result_ty)?;

        Ok(Term::case(
            scrutinee, left_var, left_body, right_var, right_body,
        ))
    }

    /// Handle ADT with exactly one constructor.
    ///
    /// No case expression needed - just bind the variable to the scrutinee.
    fn build_single_ctor_match(
        &mut self,
        scrutinee: Term,
        payload_ty: &Type,
        ctx: &AdtCodegenCtx,
        result_ty: Option<&Type>,
    ) -> ElabResult<Term> {
        let (raw_var, body) =
            self.elaborate_ctor_arm(ctx, payload_ty, &ctx.constructors[0], 0, result_ty)?;
        Ok(Term::let_in(&raw_var, payload_ty.clone(), scrutinee, body))
    }

    /// Handle the last constructor in a multi-constructor ADT.
    ///
    /// No more right branches to peel - bind variable to scrutinee.
    fn build_last_ctor_match(
        &mut self,
        scrutinee: Term,
        payload_ty: &Type,
        ctx: &AdtCodegenCtx,
        ctor_index: usize,
        result_ty: Option<&Type>,
    ) -> ElabResult<Term> {
        let (raw_var, body) = self.elaborate_ctor_arm(
            ctx,
            payload_ty,
            &ctx.constructors[ctor_index],
            ctor_index,
            result_ty,
        )?;
        Ok(Term::let_in(&raw_var, payload_ty.clone(), scrutinee, body))
    }

    /// Extract left and right types from a Sum type.
    fn extract_sum_types<'b>(
        &self,
        sum_type: &'b Type,
        ctx: &AdtCodegenCtx,
        ctor_index: usize,
        num_ctors: usize,
    ) -> ElabResult<(&'b Type, &'b Type)> {
        match sum_type {
            Type::Sum(l, r) => Ok((l.as_ref(), r.as_ref())),
            _ => {
                if debug_unfold_enabled() {
                    eprintln!("\n=== E9999 TRIGGER in build_adt_match ===");
                    eprintln!("  adt_name: {}", ctx.adt_name);
                    eprintln!("  adt_type: {:?}", ctx.adt_type);
                    eprintln!("  sum_type (expected Sum, got): {:?}", sum_type);
                    eprintln!("  ctor_index: {}", ctor_index);
                    eprintln!("  num_ctors: {}", num_ctors);
                    eprintln!(
                        "  constructors: {:?}",
                        ctx.constructors.iter().map(|c| &c.name).collect::<Vec<_>>()
                    );
                }
                Err(ElabError::new(
                    Span::new(0, 0),
                    ElabErrorKind::Other(format!(
                        "expected sum type in ADT match for {}, got {:?}",
                        ctx.adt_name, sum_type
                    )),
                ))
            }
        }
    }

    /// Elaborate a constructor arm, handling multi-arm cases with nested patterns.
    fn elaborate_ctor_arm(
        &mut self,
        ctx: &AdtCodegenCtx,
        payload_ty: &Type,
        constructor: &elab_env::Constructor,
        ctor_index: usize,
        result_ty: Option<&Type>,
    ) -> ElabResult<(String, Term)> {
        if Self::has_multiple_arms(ctx.ctor_arms, ctor_index) {
            // Multiple arms with same outer constructor - build inner match
            let arms_vec = ctx.ctor_arms.get(&ctor_index).unwrap();
            self.build_nested_match_for_arms(
                arms_vec,
                ctx.catch_all_arm,
                payload_ty,
                constructor,
                &ctx.identity(),
            )
        } else {
            let arm = Self::get_arm_for_ctor(ctx.ctor_arms, ctor_index, ctx.catch_all_arm).unwrap();

            if debug_match_enabled() && ctx.adt_name == "TokenKind" && ctor_index >= 82 {
                eprintln!(
                    "[TokenKind ctor_index={}] using arm with pattern: {:?}",
                    ctor_index, arm.pattern
                );
            }

            self.elab_ctor_arm_or_catch_all(
                arm,
                payload_ty,
                constructor,
                &ctx.identity(),
                result_ty,
            )
        }
    }

    /// Build the right branch of a case expression recursively.
    fn build_right_branch(
        &mut self,
        right_ty: &Type,
        ctx: &AdtCodegenCtx,
        ctor_index: usize,
        result_ty: &Type,
    ) -> ElabResult<(String, Term)> {
        let right_var = format!("__rest{}", ctor_index);

        self.env.push_scope();
        self.env
            .bind_local(right_var.clone(), right_ty.clone(), self.depth);
        self.depth += 1;

        let right_body = self.build_adt_match(
            Term::var(&right_var),
            right_ty,
            ctx,
            ctor_index + 1,
            result_ty,
        )?;

        self.depth -= 1;
        self.env.pop_scope();

        Ok((right_var, right_body))
    }
}
