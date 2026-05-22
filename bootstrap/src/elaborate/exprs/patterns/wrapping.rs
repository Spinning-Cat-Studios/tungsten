//! Pattern wrapping and constructor extraction.
//!
//! Functions that wrap elaborated body terms with the appropriate
//! destructions and case expressions for nested constructor patterns.

use crate::ast::Pattern;
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::env::{self as elab_env};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Context for a constructor pattern being wrapped.
///
/// Bundles the constructor identity information and the sub-pattern
/// / field type details needed to wrap a body with destructs.
pub(in crate::elaborate) struct CtorPatternCtx<'a> {
    pub type_name: &'a str,
    pub constructor: &'a elab_env::Constructor,
    pub ctor_index: usize,
    pub constructors: &'a [elab_env::Constructor],
    pub sub_patterns: &'a [Pattern],
    pub field_types: &'a [Type],
    pub value_var: &'a str,
    pub value_ty: &'a Type,
}

/// Target information for constructor extraction from a sum type.
///
/// Bundles the target constructor index, total count, and the
/// variable / body to bind when the target branch is reached.
pub(in crate::elaborate) struct ExtractionTarget {
    pub target_index: usize,
    pub num_ctors: usize,
    pub target_var: String,
    pub target_body: Term,
}

impl<'a> Elaborator<'a> {
    /// Wrap a body term with the destructs and cases for a nested constructor pattern.
    pub(in crate::elaborate) fn wrap_nested_ctor_pattern(
        &mut self,
        body: Term,
        ctor_ctx: &CtorPatternCtx,
        depth: usize,
        pattern_span: Span,
    ) -> ElabResult<Term> {
        let num_ctors = ctor_ctx.constructors.len();

        // Check if it's a recursive type (μ-type) - need to unfold first.
        // Use the ADT type name, not the constructor name, for the recursiveness check.
        let is_recursive = self.adt_is_recursive(ctor_ctx.type_name, ctor_ctx.constructors);

        // Get the unfolded sum type
        let sum_type = if is_recursive {
            match ctor_ctx.value_ty {
                Type::Mu(var, body) => body.substitute(var, ctor_ctx.value_ty),
                _ => ctor_ctx.value_ty.clone(),
            }
        } else {
            ctor_ctx.value_ty.clone()
        };

        // The value we'll match on (possibly unfolded)
        let match_value = if is_recursive {
            Term::unfold(ctor_ctx.value_ty.clone(), Term::var(ctor_ctx.value_var))
        } else {
            Term::var(ctor_ctx.value_var)
        };

        // Get the type at this constructor's position in the sum
        let ctor_ty = self.get_sum_component(&sum_type, ctor_ctx.ctor_index, num_ctors)?;

        // Create a fresh variable for the matched constructor's payload
        let raw_var = format!("__nest{}_{}", depth, ctor_ctx.constructor.name);

        // Build the body with sub-pattern bindings
        let body_with_bindings = if ctor_ctx.sub_patterns.is_empty() {
            body
        } else if ctor_ctx.sub_patterns.len() == 1 {
            self.wrap_single_subpattern(
                &ctor_ctx.sub_patterns[0],
                &ctor_ctx.field_types[0],
                &raw_var,
                body,
                depth,
            )?
        } else {
            self.wrap_product_subpatterns(
                ctor_ctx.sub_patterns,
                ctor_ctx.field_types,
                &raw_var,
                body,
                depth,
            )?
        };

        // Build the case expression that matches this constructor
        if num_ctors == 1 {
            Ok(Term::let_in(
                &raw_var,
                ctor_ty.clone(),
                match_value,
                body_with_bindings,
            ))
        } else {
            self.build_ctor_extraction(
                match_value,
                &sum_type,
                ExtractionTarget {
                    target_index: ctor_ctx.ctor_index,
                    num_ctors,
                    target_var: raw_var,
                    target_body: body_with_bindings,
                },
                pattern_span,
            )
        }
    }

    /// Wrap body with binding for a single sub-pattern.
    pub(in crate::elaborate) fn wrap_single_subpattern(
        &mut self,
        pattern: &Pattern,
        pattern_ty: &Type,
        value_var: &str,
        body: Term,
        depth: usize,
    ) -> ElabResult<Term> {
        match pattern {
            Pattern::Wildcard(_) => Ok(body),
            Pattern::Var(ref var) => Ok(Term::let_in(
                &var.name,
                pattern_ty.clone(),
                Term::var(value_var),
                body,
            )),
            Pattern::Constructor(ref ctor_path, ref sub_patterns, _) => {
                // Nested constructor - wrap recursively
                let resolved = self.resolve_pattern_ctor(ctor_path, pattern.span())?;
                let constructor = &resolved.constructors[resolved.index];

                // Instantiate constructor field types with proper two-phase substitution
                // (type params first, then μ-type unfolding - see ADR 24.1.26, ADR 30.1.26)
                // Use explicit ADT name for non-recursive types like Option
                let field_types = self.instantiate_constructor_fields_with_name(
                    &constructor.fields,
                    &resolved.type_params,
                    pattern_ty,
                    &resolved.type_name,
                );

                let ctor_ctx = CtorPatternCtx {
                    type_name: &resolved.type_name,
                    constructor,
                    ctor_index: resolved.index,
                    constructors: &resolved.constructors,
                    sub_patterns,
                    field_types: &field_types,
                    value_var,
                    value_ty: pattern_ty,
                };

                self.wrap_nested_ctor_pattern(body, &ctor_ctx, depth + 1, pattern.span())
            }
            Pattern::Tuple(ref sub_pats, tup_span) => {
                // Tuple inside constructor (ADR 15.5.26f) — wrap with
                // tuple projection lets using right-nested convention.
                let elem_types = self.extract_tuple_types(pattern_ty, sub_pats.len(), *tup_span)?;
                self.build_tuple_lets(sub_pats, &elem_types, value_var, body, *tup_span)
            }
            _ => Err(ElabError::unsupported(pattern.span(), "this pattern kind")),
        }
    }

    /// Wrap body with bindings for product (multi-field) sub-patterns.
    pub(in crate::elaborate) fn wrap_product_subpatterns(
        &mut self,
        patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: Term,
        depth: usize,
    ) -> ElabResult<Term> {
        let n = patterns.len();
        let mut result = body;

        // Process patterns in reverse order (innermost bindings first)
        for i in (0..n).rev() {
            // Build the accessor for field i
            let field_var = format!("__field{}_{}", depth, i);
            let mut accessor = Term::var(raw_var);
            for _ in 0..i {
                accessor = Term::snd(accessor);
            }
            if i < n - 1 {
                accessor = Term::fst(accessor);
            }

            // Wrap with pattern binding
            result = self.wrap_single_subpattern(
                &patterns[i],
                &field_types[i],
                &field_var,
                result,
                depth,
            )?;

            // Wrap with let binding for the field accessor
            result = Term::let_in(&field_var, field_types[i].clone(), accessor, result);
        }

        Ok(result)
    }

    /// Build a case expression that extracts a specific constructor from a sum type.
    ///
    /// For the non-matching branches, generates absurd (bottom elimination).
    ///
    /// ## Representation Policy (ADR 2.2.26)
    ///
    /// - n = 1: Single constructor, just let binding
    /// - n = 2: Binary sum, nested case expressions
    /// - n >= 3: Flat ADT, use Term::adt_match direct extraction
    pub(in crate::elaborate) fn build_ctor_extraction(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        target: ExtractionTarget,
        span: Span,
    ) -> ElabResult<Term> {
        // ADR 2.2.26: For flat ADT (n >= 3), use adt_match directly
        if let Type::Adt(_, _, variants) = sum_type {
            // Get the payload type for this variant
            let _payload_ty = variants
                .get(target.target_index)
                .map(|(_, ty)| ty.clone())
                .unwrap_or(Type::Unit);

            // Build arms: target arm returns body, others return Sorry (unreachable)
            let arms: Vec<(usize, String, Box<Term>)> = (0..target.num_ctors)
                .map(|idx| {
                    if idx == target.target_index {
                        (
                            idx,
                            target.target_var.to_string(),
                            Box::new(target.target_body.clone()),
                        )
                    } else {
                        // Other arms are unreachable in a let pattern
                        let dummy_var = format!("__unreachable_{}", idx);
                        (idx, dummy_var, Box::new(Term::Sorry))
                    }
                })
                .collect();

            return Ok(Term::adt_match(scrutinee, arms));
        }

        self.build_ctor_extraction_at(scrutinee, sum_type, target, 0, span)
    }

    /// Recursive helper for build_ctor_extraction.
    fn build_ctor_extraction_at(
        &mut self,
        scrutinee: Term,
        sum_type: &Type,
        target: ExtractionTarget,
        current_index: usize,
        span: Span,
    ) -> ElabResult<Term> {
        if current_index == target.num_ctors - 1 {
            // Last position: this must be our target (no more rights to peel)
            if current_index != target.target_index {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other(
                        "internal error: reached end of sum without finding target constructor"
                            .to_string(),
                    ),
                ));
            }
            return Ok(Term::let_in(
                &target.target_var,
                sum_type.clone(),
                scrutinee,
                target.target_body,
            ));
        }

        // Unwrap Mu layers if present (including nested Mu from mutual recursion)
        let unfolded;
        let unwrapped = if matches!(sum_type, Type::Mu(_, _)) {
            unfolded = self.unfold_inner_mu_layers(sum_type.clone());
            &unfolded
        } else {
            sum_type
        };

        // Get the left and right types of the current sum
        let (_left_ty, right_ty) = match unwrapped {
            Type::Sum(l, r) => (&**l, &**r),
            _ => {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other("expected sum type in constructor extraction".to_string()),
                ))
            }
        };

        if current_index == target.target_index {
            // This is our target constructor (the left branch)
            let right_var = format!("__abs{}", current_index);
            // For the right branch, we generate absurd (this case shouldn't happen
            // if pattern matching is correct, but we need a term)
            // We use a sorry/hole as a placeholder
            let absurd_body = Term::Sorry; // Placeholder, won't be evaluated

            Ok(Term::case(
                scrutinee,
                &target.target_var,
                target.target_body,
                right_var,
                absurd_body,
            ))
        } else {
            // Target is in the right branch
            let left_var = format!("__abs{}", current_index);
            let absurd_body = Term::Sorry;

            let right_var = format!("__rest{}", current_index);

            // Recursively build the right branch
            let right_body = self.build_ctor_extraction_at(
                Term::var(&right_var),
                right_ty,
                target,
                current_index + 1,
                span,
            )?;

            Ok(Term::case(
                scrutinee,
                left_var,
                absurd_body,
                right_var,
                right_body,
            ))
        }
    }
}
