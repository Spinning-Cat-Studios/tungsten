//! Sum type injection building.
//!
//! Builds the injection chain (inl/inr) for constructor values.

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;
use tungsten_core::{Term, Type};

impl<'a> Elaborator<'a> {
    /// Build the injection chain for a constructor value.
    /// For constructor at index i out of n constructors:
    /// - index 0: inl(value) into sum type
    /// - index 1: inr(inl(value))
    /// - index 2: inr(inr(inl(value)))
    /// etc.
    ///
    /// # Cross-Module Type Handling (ADR 30.1.26 Category A Fix)
    ///
    /// When the ADT type comes from a cross-module reference, it may be
    /// represented as `Type::App("TypeName", [])` instead of its structural
    /// encoding. We normalize the type first to expand such references.
    pub(in crate::elaborate) fn build_constructor_injection(
        &self,
        value: Term,
        index: usize,
        num_ctors: usize,
        adt_type: &Type,
    ) -> ElabResult<Term> {
        if num_ctors == 0 {
            return Err(ElabError::new(
                Span::new(0, 0),
                ElabErrorKind::Other("cannot build injection for empty ADT".to_string()),
            ));
        }

        if num_ctors == 1 {
            // Single constructor: no injection needed, value is the type
            return Ok(value);
        }

        // Normalize the ADT type to expand cross-module Type::App references
        // to their structural encodings. This handles the case where adt_type
        // is Type::App("SomeType", []) from a cross-module reference.
        let normalized_adt = self.normalize_for_comparison(adt_type);

        // Get the unfolded type (if μ-type, get the body)
        let sum_type = match &normalized_adt {
            Type::Mu(_, body) => (**body).clone(),
            _ => normalized_adt.clone(),
        };

        // Build from inside out for right-nested sum: A + (B + (C + D))
        // index 0 (A): inl(value)                            at type A + (B + (C + D))
        // index 1 (B): inr(inl(value))                       at type A + (B + (C + D))
        // index 2 (C): inr(inr(inl(value)))                  at type A + (B + (C + D))
        // index 3 (D): inr(inr(inr(value)))                  at type A + (B + (C + D))
        //
        // Pattern: wrap in (index) inr's, then if not last, wrap in inl

        let mut result = value;

        // Collect sum types while descending to the target level
        // sum_types[0] = outermost sum type
        // sum_types[index] = target level (innermost for this constructor)
        let mut sum_types = vec![sum_type.clone()];
        let mut current = sum_type.clone();
        for _ in 0..index {
            current = match &current {
                Type::Sum(_, right) => (**right).clone(),
                _ => {
                    return Err(ElabError::new(
                        Span::new(0, 0),
                        ElabErrorKind::Other(
                            "expected sum type in constructor injection".to_string(),
                        ),
                    ));
                }
            };
            sum_types.push(current.clone());
        }

        // If not the last constructor, wrap in inl at the target level
        // target level is sum_types[index]
        if index < num_ctors - 1 {
            result = Term::inl(sum_types[index].clone(), result);
        }

        // Now wrap in inr's from inside out, going back up the sum chain
        // Start from the innermost wrapping (just above target) and work outward
        // After all wrappings, result has type sum_types[0] (the outermost sum)
        for i in (0..index).rev() {
            result = Term::inr(sum_types[i].clone(), result);
        }

        Ok(result)
    }
}
