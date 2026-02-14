//! ADT match expression elaboration.
//!
//! This module handles pattern matching on algebraic data types (ADTs).
//!
//! ## Architecture
//!
//! The elaboration is split into phases for clarity:
//!
//! 1. **Resolution** (`resolution`) - Resolves the ADT type from constructor patterns
//! 2. **Classification** (`classification`) - Groups arms by constructor vs catch-all
//! 3. **Type unfolding** (`unfolding`) - Handles μ-type unfolding for recursive ADTs
//! 4. **Type inference** (`inference`) - Determines the result type
//! 5. **Code generation** (`codegen`) - Generates nested case expressions
//!
//! ## Key Types
//!
//! - `AdtMatchContext` - Holds resolved ADT info (type def, constructors, recursiveness)
//! - `ClassifiedArms` - Arms grouped by constructor index, plus optional catch-all
//!
//! ## Testing
//!
//! Unit tests focus on the two-phase substitution bug (ADR 30.1.26):
//! - Generic ADT field types require substituting type params THEN μ-refs
//! - Missing Phase 1 causes leaked `α_List` variables in error messages

mod arm_elaboration;
mod classification;
mod codegen;
mod context;
mod inference;
mod resolution;
mod unfolding;

#[cfg(test)]
mod tests;

// Re-export the context types
pub(super) use context::ClassifiedArms;

use crate::ast;
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

// ============================================================================
// Main Entry Point
// ============================================================================

impl<'a> Elaborator<'a> {
    /// Elaborate match on an ADT with constructor patterns.
    ///
    /// This is the main entry point for ADT pattern matching. It coordinates
    /// the phases of elaboration: resolution, classification, type inference,
    /// and code generation.
    ///
    /// ## Representation Policy (ADR 2.2.26)
    ///
    /// - n = 1: Single constructor, no case needed
    /// - n = 2: Binary sum → nested Term::case
    /// - n >= 3: Flat ADT → Term::adt_match with O(1) switch dispatch
    pub(super) fn elab_adt_match(
        &mut self,
        scrutinee_term: Term,
        scrutinee_ty: Type,
        arms: &[ast::MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        // Phase 1: Resolve ADT context from constructor patterns
        let ctx = self.resolve_adt_match_context(arms, span)?;
        let num_ctors = ctx.constructors.len();

        // Phase 2: Unfold the scrutinee type if recursive
        let (inner_type, match_scrutinee) =
            self.unfold_scrutinee_type(&scrutinee_ty, scrutinee_term, ctx.is_recursive);

        // Phase 3: Classify arms by constructor vs catch-all
        let classified = self.classify_match_arms(arms, &ctx.constructors, span)?;

        // Phase 4: Check exhaustiveness
        self.check_exhaustiveness(&classified, num_ctors, span)?;

        // Phase 5: Determine result type
        let result_ty = self.infer_match_result_type(
            expected,
            &classified,
            &inner_type,
            &ctx.constructors,
            &scrutinee_ty,
            &ctx.type_def.params,
        )?;

        // Phase 6: Build case analysis
        // Route based on representation policy (ADR 2.2.26)
        let term = if num_ctors >= 3 {
            // Flat ADT: use Term::adt_match with O(1) switch
            self.build_flat_adt_match(
                match_scrutinee,
                &classified.ctor_arms,
                classified.catch_all,
                &ctx.constructors,
                &result_ty,
                &scrutinee_ty,
                &ctx.type_def.params,
                &ctx.type_def.name,
            )?
        } else {
            // Binary sum: use nested Term::case (existing code)
            self.build_adt_match(
                match_scrutinee,
                &inner_type,
                &classified.ctor_arms,
                classified.catch_all,
                &ctx.constructors,
                0,
                &result_ty,
                &scrutinee_ty,
                &ctx.type_def.params,
                &ctx.type_def.name,
            )?
        };

        Ok((term, result_ty))
    }

    /// Check that the match is exhaustive.
    fn check_exhaustiveness(
        &self,
        classified: &ClassifiedArms,
        num_ctors: usize,
        span: Span,
    ) -> ElabResult<()> {
        if classified.catch_all.is_none() && classified.ctor_arms.len() != num_ctors {
            return Err(
                ElabError::new(span, ElabErrorKind::NonExhaustiveMatch).with_note(format!(
                    "expected {} arms for {} constructors, or use a catch-all `_` pattern",
                    num_ctors, num_ctors
                )),
            );
        }
        Ok(())
    }
}
