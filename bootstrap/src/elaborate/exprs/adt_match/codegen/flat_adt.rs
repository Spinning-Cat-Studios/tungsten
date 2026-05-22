//! Flat ADT match generation for types with 3+ constructors (ADR 2.2.26).
//!
//! Generates `Term::adt_match` with O(1) switch dispatch instead of
//! nested O(n) case expressions.

use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

use super::debug::debug_match_enabled;
use crate::elaborate::exprs::adt_match::context::AdtCodegenCtx;

impl<'a> Elaborator<'a> {
    /// Build flat ADT match for types with 3+ constructors (ADR 2.2.26).
    ///
    /// Generates `Term::adt_match` with O(1) switch dispatch instead of
    /// nested O(n) case expressions.
    ///
    /// # Arguments
    /// - `scrutinee`: The term being matched (must be Type::Adt)
    /// - `ctor_arms`: Map of constructor index → specific arms (may have multiple)
    /// - `catch_all_arm`: Optional default arm for unmatched constructors
    /// - `constructors`: All constructors for this ADT
    /// - `result_ty`: Expected result type of the match
    /// - `adt_type`: The ADT type (for recursive references and field type instantiation)
    /// - `type_params`: Type parameters for generic substitution
    /// - `adt_name`: Name of the ADT
    pub(in crate::elaborate::exprs::adt_match) fn build_flat_adt_match(
        &mut self,
        scrutinee: Term,
        ctx: &AdtCodegenCtx,
        result_ty: &Type,
    ) -> ElabResult<Term> {
        let num_ctors = ctx.constructors.len();

        if debug_match_enabled() {
            eprintln!(
                "\n=== Flat ADT Match for {} ({} constructors) ===",
                ctx.adt_name, num_ctors
            );
            eprintln!(
                "ctor_arms keys: {:?}",
                ctx.ctor_arms.keys().collect::<Vec<_>>()
            );
            eprintln!("catch_all_arm: {:?}", ctx.catch_all_arm.is_some());
        }

        // Build arms for all constructors
        let mut arms: Vec<(usize, String, Box<Term>)> = Vec::with_capacity(num_ctors);

        for (idx, ctor) in ctx.constructors.iter().enumerate() {
            // Get the payload type for this constructor
            let field_types = self.instantiate_constructor_fields_with_name(
                &ctor.fields,
                ctx.type_params,
                ctx.adt_type,
                ctx.adt_name,
            );

            // The payload type is the product of all fields (or Unit if nullary)
            let payload_ty = if field_types.is_empty() {
                Type::Unit
            } else if field_types.len() == 1 {
                field_types[0].clone()
            } else {
                // Left-nested product: ((a, b), c)
                let mut iter = field_types.iter();
                let mut product = iter.next().unwrap().clone();
                for ty in iter {
                    product = Type::product(product, ty.clone());
                }
                product
            };

            // Handle multiple arms for same constructor vs single arm vs catch-all
            let (var, body) = if Self::has_multiple_arms(ctx.ctor_arms, idx) {
                let arms_vec = ctx.ctor_arms.get(&idx).unwrap();
                self.build_nested_match_for_arms(
                    arms_vec,
                    ctx.catch_all_arm,
                    &payload_ty,
                    ctor,
                    &ctx.identity(),
                )?
            } else {
                let arm = Self::get_arm_for_ctor(ctx.ctor_arms, idx, ctx.catch_all_arm)
                    .ok_or_else(|| {
                        ElabError::new(Span::new(0, 0), ElabErrorKind::NonExhaustiveMatch)
                            .with_note(format!(
                                "no pattern for constructor {} at index {}",
                                ctor.name, idx
                            ))
                    })?;

                self.elab_ctor_arm_or_catch_all(
                    arm,
                    &payload_ty,
                    ctor,
                    &ctx.identity(),
                    Some(result_ty),
                )?
            };

            if debug_match_enabled() && ctx.adt_name == "TokenKind" {
                eprintln!("  [{}] {} -> var={}", idx, ctor.name, var);
            }

            arms.push((idx, var, Box::new(body)));
        }

        // Create the Term::adt_match
        Ok(Term::AdtMatch(Box::new(scrutinee), arms))
    }
}
