//! Product destructuring for multi-field constructor patterns.
//!
//! Handles binding and wrapping pattern variables from a constructor's
//! right-nested payload product when it has multiple fields (ADR 1.8.26b D1).

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
        // For patterns [a, b, c] from right-nested product (a, (b, c)):
        // let a = fst(raw); let b = fst(snd(raw)); let c = snd(snd(raw)); body
        let mut result = body;
        let n = patterns.len();

        for i in (0..n).rev() {
            let Pattern::Var(ref var) = patterns[i] else {
                continue;
            };

            let accessor = Self::build_payload_accessor(raw_var, i, n);

            result = Term::let_in(&var.name, field_types[i].clone(), accessor, result);
        }

        // Decrement depth for each pattern we bound
        for _ in 0..n {
            self.depth -= 1;
        }

        Ok(result)
    }

    /// Build the accessor for field `field_idx` of a constructor payload with
    /// `num_fields` fields.
    ///
    /// Right-nested encoding: `(a, (b, c))` for `[a, b, c]` (ADR 1.8.26b D1) —
    /// the shape `build_product_value` builds, `ctor_fields_product` encodes,
    /// and `wrapping.rs` already projected:
    /// - Field 0: `fst(raw)`
    /// - Field 1: `fst(snd(raw))`
    /// - Field 2: `snd(snd(raw))`
    ///
    /// This projected the *left*-nested shape until 1.8.26b, so a `match` on
    /// any ≥3-field constructor emitted `Fst(Fst(v))` against a right-nested
    /// value and went silently Stuck — a miscompile wider than the comparator
    /// defect that exposed it.
    pub(super) fn build_payload_accessor(
        raw_var: &str,
        field_idx: usize,
        num_fields: usize,
    ) -> Term {
        let mut accessor = Term::var(raw_var);
        for _ in 0..field_idx {
            accessor = Term::snd(accessor);
        }
        if field_idx + 1 < num_fields {
            accessor = Term::fst(accessor);
        }
        accessor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elaborate::Elaborator;

    /// Apply the accessor to a concrete right-nested payload and reduce it, so
    /// the test asserts the field it *retrieves* rather than the term shape it
    /// happens to build. A shape assertion would pass just as well against the
    /// wrong nesting spelled consistently.
    fn project(payload: &[u64], field_idx: usize) -> u64 {
        let value = build_right_nested_value(payload);
        let accessor = Elaborator::build_payload_accessor("v", field_idx, payload.len());
        let mut env = std::collections::HashMap::new();
        env.insert("v".to_string(), value);
        let term = accessor.substitute("v", &env["v"].clone());
        reduce_to_nat(&term).expect("the accessor must reduce to a field")
    }

    fn build_right_nested_value(fields: &[u64]) -> Term {
        let mut iter = fields.iter().rev();
        let mut value = Term::NatLit(*iter.next().expect("non-empty payload"));
        for f in iter {
            value = Term::pair(Term::NatLit(*f), value);
        }
        value
    }

    fn reduce_to_nat(term: &Term) -> Option<u64> {
        match term {
            Term::NatLit(n) => Some(*n),
            Term::Fst(inner) => reduce_to_nat(&reduce_pair(inner)?.0),
            Term::Snd(inner) => reduce_to_nat(&reduce_pair(inner)?.1),
            _ => None,
        }
    }

    fn reduce_pair(term: &Term) -> Option<(Term, Term)> {
        match term {
            Term::Pair(a, b) => Some((a.as_ref().clone(), b.as_ref().clone())),
            Term::Fst(inner) => reduce_pair(&reduce_pair(inner)?.0),
            Term::Snd(inner) => reduce_pair(&reduce_pair(inner)?.1),
            _ => None,
        }
    }

    #[test]
    fn a_single_field_payload_is_the_value_itself() {
        assert_eq!(project(&[7], 0), 7);
    }

    #[test]
    fn a_two_field_payload_projects_both_fields() {
        assert_eq!(project(&[10, 20], 0), 10);
        assert_eq!(project(&[10, 20], 1), 20);
    }

    /// Three is the exact boundary: below it left- and right-nesting coincide,
    /// so a wrong accessor is invisible (ADR 1.8.26b D1).
    #[test]
    fn a_three_field_payload_projects_first_middle_and_last() {
        assert_eq!(project(&[1, 2, 3], 0), 1);
        assert_eq!(project(&[1, 2, 3], 1), 2);
        assert_eq!(project(&[1, 2, 3], 2), 3);
    }

    #[test]
    fn a_seven_field_payload_projects_every_field() {
        let payload: Vec<u64> = (10..17).collect();
        for (i, expected) in payload.iter().enumerate() {
            assert_eq!(project(&payload, i), *expected, "field {i}");
        }
    }

    /// The pre-1.8.26b accessor, kept as an explicit negative: it retrieved the
    /// wrong field (or nothing at all) from a right-nested value, which is the
    /// miscompile this change removes.
    #[test]
    fn the_left_nested_accessor_would_not_project_correctly() {
        let value = build_right_nested_value(&[1, 2, 3]);
        // Field 0 under the old convention: fst(fst(v)).
        let old = Term::fst(Term::fst(value));
        assert_eq!(
            reduce_to_nat(&old),
            None,
            "fst of a scalar cannot reduce — this is exactly the Stuck term"
        );
    }
}
