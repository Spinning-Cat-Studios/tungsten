//! Shared helper functions for constructor elaboration.

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;
use tungsten_core::{Term, Type};

impl<'a> Elaborator<'a> {
    /// Validate constructor arity matches expected.
    pub(crate) fn validate_ctor_arity(
        &self,
        name: &str,
        expected: usize,
        found: usize,
        span: Span,
    ) -> ElabResult<()> {
        if expected != found {
            return Err(
                ElabError::new(span, ElabErrorKind::ArityMismatch { expected, found }).with_note(
                    format!(
                        "constructor `{}` takes {} argument(s), but {} were provided",
                        name, expected, found
                    ),
                ),
            );
        }
        Ok(())
    }

    /// Build a product term from argument terms.
    /// Single arg returns as-is, multiple args build right-nested pairs.
    pub(crate) fn build_product_value(&self, arg_terms: Vec<Term>) -> Term {
        if arg_terms.is_empty() {
            Term::Unit
        } else if arg_terms.len() == 1 {
            arg_terms.into_iter().next().unwrap()
        } else {
            // Multiple fields: build right-nested pairs (a, (b, (c, ...)))
            let mut iter = arg_terms.into_iter().rev();
            let mut product = iter.next().unwrap();
            for term in iter {
                product = Term::pair(term, product);
            }
            product
        }
    }

    /// Wrap a term in a fold if the ADT is recursive.
    /// Normalizes the adt_type to its Mu encoding so codegen receives
    /// a proper recursive type instead of an App reference.
    pub(crate) fn wrap_in_fold_if_recursive(
        &self,
        term: Term,
        adt_type: Type,
        is_recursive: bool,
    ) -> Term {
        if is_recursive {
            let normalized = self.normalize_for_comparison(&adt_type);

            // --trace-types instrumentation point 4: wrap_in_fold (ADR 13.4.26c §5)
            if self.should_trace() {
                self.trace(
                    "wrap_in_fold",
                    &format!(
                        "adt_type: {}\nnormalized: {}\nis_recursive: true",
                        self.format_type_with_provenance(&adt_type),
                        self.format_type_with_provenance(&normalized)
                    ),
                );
            }

            Term::fold(normalized, term)
        } else {
            term
        }
    }

    /// Build the full constructor term: value → injection → fold.
    ///
    /// Policy (ADR 2.2.26):
    /// - n = 1: no injection needed
    /// - n = 2: binary sum injection (inl/inr)
    /// - n >= 3: flat ADT construction (Term::adt_construct)
    pub(crate) fn build_constructor_term(
        &self,
        value: Term,
        index: usize,
        num_ctors: usize,
        adt_type: &Type,
        is_recursive: bool,
    ) -> ElabResult<Term> {
        // For 3+ constructors, use flat ADT representation
        if num_ctors >= 3 {
            // For flat ADT, we emit Term::adt_construct directly
            // The adt_type should be Type::Adt for n >= 3
            let result = Term::adt_construct(adt_type.clone(), index, value);
            // Note: For flat ADT, recursive wrapping is handled differently
            // The Type::Adt already captures the structure
            return Ok(if is_recursive {
                let normalized = self.normalize_for_comparison(adt_type);
                Term::fold(normalized, result)
            } else {
                result
            });
        }

        // For 1-2 constructors, use existing binary sum injection
        let injected = self.build_constructor_injection(value, index, num_ctors, adt_type)?;
        Ok(self.wrap_in_fold_if_recursive(injected, adt_type.clone(), is_recursive))
    }
}
