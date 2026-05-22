//! Product destructuring for multi-field constructor patterns.
//!
//! Handles binding and wrapping pattern variables from left-nested products
//! when a constructor has multiple fields.

use crate::ast::Pattern;
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate multiple field patterns (product destructuring).
    pub(super) fn elab_multi_field_patterns(
        &mut self,
        sub_patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: &crate::ast::Expr,
        result_ty: Option<&Type>,
    ) -> ElabResult<Term> {
        let has_nested_complex = sub_patterns
            .iter()
            .any(|p| matches!(p, Pattern::Constructor(_, _, _) | Pattern::Tuple(_, _)));

        if has_nested_complex {
            // Use recursive pattern elaboration for nested constructors/tuples
            self.elab_product_with_nested_ctors(sub_patterns, field_types, raw_var, body, 2)
        } else {
            // Use simpler approach for vars and wildcards
            self.bind_product_patterns(sub_patterns, field_types, raw_var)?;
            let body_term = if let Some(expected) = result_ty {
                self.check(body, expected)?
            } else {
                self.infer(body)?.0
            };
            self.wrap_product_destructs(body_term, sub_patterns, field_types, raw_var)
        }
    }

    /// Bind pattern variables from a product (for multi-field constructors).
    /// Wildcards (`_`) are skipped - no binding is created.
    pub(super) fn bind_product_patterns(
        &mut self,
        patterns: &[Pattern],
        field_types: &[Type],
        _raw_var: &str,
    ) -> ElabResult<()> {
        for (pat, ty) in patterns.iter().zip(field_types.iter()) {
            match pat {
                Pattern::Wildcard(_) => {
                    // Wildcard: skip binding, but still increment depth for tracking
                    self.depth += 1;
                }
                Pattern::Var(ref var) => {
                    self.env
                        .bind_local(var.name.clone(), ty.clone(), self.depth);
                    self.depth += 1;
                }
                _ => {
                    return Err(ElabError::unsupported(
                        pat.span(),
                        "nested patterns in constructors",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Wrap body with product destructuring lets.
    pub(super) fn wrap_product_destructs(
        &mut self,
        body: Term,
        patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
    ) -> ElabResult<Term> {
        // For patterns [a, b, c] from left-nested product ((a, b), c):
        // let a = fst(fst(raw)); let b = snd(fst(raw)); let c = snd(raw); body
        let mut result = body;
        let n = patterns.len();

        for i in (0..n).rev() {
            let Pattern::Var(ref var) = patterns[i] else {
                continue;
            };

            // Build the accessor for field i using left-nested product convention
            let accessor = Self::build_left_nested_accessor(raw_var, i, n);

            result = Term::let_in(&var.name, field_types[i].clone(), accessor, result);
        }

        // Decrement depth for each pattern we bound
        for _ in 0..n {
            self.depth -= 1;
        }

        Ok(result)
    }

    /// Build accessor for field at index `field_idx` in a left-nested product of `num_fields` fields.
    ///
    /// Left-nested encoding: ((a, b), c) for [a, b, c]
    /// - Field 0: fst(fst(raw))
    /// - Field 1: snd(fst(raw))
    /// - Field 2: snd(raw)
    pub(super) fn build_left_nested_accessor(
        raw_var: &str,
        field_idx: usize,
        num_fields: usize,
    ) -> Term {
        fn helper(raw: Term, field_idx: usize, num_fields: usize) -> Term {
            if num_fields == 1 {
                raw
            } else if field_idx == num_fields - 1 {
                Term::snd(raw)
            } else {
                helper(Term::fst(raw), field_idx, num_fields - 1)
            }
        }
        helper(Term::var(raw_var), field_idx, num_fields)
    }
}
