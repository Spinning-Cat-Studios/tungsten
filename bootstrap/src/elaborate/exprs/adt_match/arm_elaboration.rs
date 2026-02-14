//! Arm Elaboration
//!
//! Elaborates individual match arms with constructor patterns.
//!
//! ## Two-Phase Substitution (ADR 30.1.26)
//!
//! When elaborating constructor arms, field types must be correctly substituted:
//!
//! 1. **Phase 1**: Type parameters → concrete types (e.g., `T` → `String`)
//! 2. **Phase 2**: μ-variables → full μ-type (e.g., `α_List` → `μα_List. ...`)
//!
//! The `instantiate_constructor_fields` helper performs both phases.

use crate::ast::{self, Pattern};
use crate::span::Spanned;
use tungsten_core::{Term, Type};

use crate::elaborate::env::{self as elab_env};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate a match arm - either a constructor pattern or a catch-all (wildcard/variable).
    pub(super) fn elab_ctor_arm_or_catch_all(
        &mut self,
        arm: &ast::MatchArm,
        ctor_ty: &Type, // The type of the value at this position in the sum
        constructor: &elab_env::Constructor,
        adt_type: &Type,        // Original ADT type for recursive references
        type_params: &[String], // Type parameters of the ADT (for generic substitution)
        adt_name: &str,         // Name of the ADT (for type argument extraction)
    ) -> ElabResult<(String, Term)> {
        match &arm.pattern {
            Pattern::Constructor(_, _, _) => {
                self.elab_ctor_arm(arm, ctor_ty, constructor, adt_type, type_params, adt_name)
            }
            Pattern::Wildcard(_) => self.elab_wildcard_arm(&arm.body, ctor_ty, &constructor.name),
            Pattern::Var(ref var) => self.elab_var_arm(&arm.body, ctor_ty, &var.name),
            _ => Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::Other(
                    "expected constructor, wildcard, or variable pattern".to_string(),
                ),
            )),
        }
    }

    /// Elaborate a wildcard catch-all pattern.
    /// Binds the value to a fresh variable but doesn't use it in the body.
    fn elab_wildcard_arm(
        &mut self,
        body: &ast::Expr,
        ctor_ty: &Type,
        ctor_name: &str,
    ) -> ElabResult<(String, Term)> {
        let raw_var = format!("__catch_all_{}", ctor_name);
        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            let body_term = elab.infer(body)?.0;
            Ok((raw_var.clone(), body_term))
        })
    }

    /// Elaborate a variable catch-all pattern.
    /// Binds the entire value to the variable name.
    fn elab_var_arm(
        &mut self,
        body: &ast::Expr,
        ctor_ty: &Type,
        var_name: &str,
    ) -> ElabResult<(String, Term)> {
        let raw_var = var_name.to_string();
        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            let body_term = elab.infer(body)?.0;
            Ok((raw_var.clone(), body_term))
        })
    }

    /// Execute a closure with a scoped variable binding.
    /// Handles push/pop scope and depth management.
    fn with_scoped_binding<T, F>(&mut self, name: &str, ty: Type, f: F) -> ElabResult<T>
    where
        F: FnOnce(&mut Self) -> ElabResult<T>,
    {
        self.env.push_scope();
        self.env.bind_local(name.to_string(), ty, self.depth);
        self.depth += 1;

        let result = f(self);

        self.depth -= 1;
        self.env.pop_scope();

        result
    }

    /// Elaborate a match arm with a constructor pattern.
    ///
    /// This function uses `instantiate_constructor_fields` for correct two-phase
    /// substitution of field types. See ADR 30.1.26 for details.
    pub(super) fn elab_ctor_arm(
        &mut self,
        arm: &ast::MatchArm,
        ctor_ty: &Type, // The type of the value at this position in the sum
        constructor: &elab_env::Constructor,
        adt_type: &Type,        // Original ADT type for recursive references
        type_params: &[String], // Type parameters of the ADT (for generic substitution)
        adt_name: &str,         // Name of the ADT (for type argument extraction)
    ) -> ElabResult<(String, Term)> {
        let Pattern::Constructor(_, ref sub_patterns, _) = arm.pattern else {
            return Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::Other("expected constructor pattern".to_string()),
            ));
        };

        // Validate the arm
        self.validate_ctor_arm(arm, constructor, sub_patterns.len())?;

        // Instantiate constructor field types with proper two-phase substitution
        let field_types = self.instantiate_constructor_fields_with_name(
            &constructor.fields,
            type_params,
            adt_type,
            adt_name,
        );

        // Create a fresh variable for the raw matched value
        let raw_var = format!("__ctor_{}", constructor.name);

        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            // Elaborate body with pattern bindings
            let body_term = elab.elab_ctor_body(sub_patterns, &field_types, &raw_var, &arm.body)?;
            Ok((raw_var.clone(), body_term))
        })
    }

    /// Validate a constructor arm: check for guards and arity.
    fn validate_ctor_arm(
        &self,
        arm: &ast::MatchArm,
        constructor: &elab_env::Constructor,
        pattern_count: usize,
    ) -> ElabResult<()> {
        if arm.guard.is_some() {
            return Err(ElabError::unsupported(arm.span, "match guards"));
        }

        if pattern_count != constructor.fields.len() {
            return Err(ElabError::new(
                arm.pattern.span(),
                ElabErrorKind::ArityMismatch {
                    expected: constructor.fields.len(),
                    found: pattern_count,
                },
            ));
        }

        Ok(())
    }

    /// Elaborate the body of a constructor arm with field bindings.
    fn elab_ctor_body(
        &mut self,
        sub_patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: &ast::Expr,
    ) -> ElabResult<Term> {
        if sub_patterns.is_empty() {
            // Nullary constructor: just elaborate the body
            self.infer(body).map(|(term, _)| term)
        } else if sub_patterns.len() == 1 {
            // Single field: handle directly
            self.elab_single_field_pattern(&sub_patterns[0], &field_types[0], raw_var, body)
        } else {
            // Multiple fields: destructure the product
            self.elab_multi_field_patterns(sub_patterns, field_types, raw_var, body)
        }
    }

    /// Elaborate a single-field pattern.
    fn elab_single_field_pattern(
        &mut self,
        pattern: &Pattern,
        field_type: &Type,
        raw_var: &str,
        body: &ast::Expr,
    ) -> ElabResult<Term> {
        match pattern {
            Pattern::Wildcard(_) => {
                // Wildcard: don't bind anything, just elaborate body
                self.infer(body).map(|(term, _)| term)
            }
            Pattern::Var(ref var) => {
                self.elab_single_var_binding(&var.name, field_type, raw_var, body)
            }
            Pattern::Constructor(_, _, _) => {
                // Nested constructor pattern: use recursive elaboration
                self.elab_nested_ctor_pattern(pattern, raw_var, field_type, body, 2)
            }
            _ => Err(ElabError::unsupported(
                pattern.span(),
                "this pattern kind in constructors",
            )),
        }
    }

    /// Elaborate a single variable binding for a field.
    fn elab_single_var_binding(
        &mut self,
        var_name: &str,
        field_type: &Type,
        raw_var: &str,
        body: &ast::Expr,
    ) -> ElabResult<Term> {
        self.env
            .bind_local(var_name.to_string(), field_type.clone(), self.depth);
        self.depth += 1;

        let body_term = self.infer(body)?.0;
        let wrapped = Term::let_in(var_name, field_type.clone(), Term::var(raw_var), body_term);

        self.depth -= 1;
        Ok(wrapped)
    }

    /// Elaborate multiple field patterns (product destructuring).
    fn elab_multi_field_patterns(
        &mut self,
        sub_patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: &ast::Expr,
    ) -> ElabResult<Term> {
        let has_nested_ctor = sub_patterns
            .iter()
            .any(|p| matches!(p, Pattern::Constructor(_, _, _)));

        if has_nested_ctor {
            // Use recursive pattern elaboration for nested constructors
            self.elab_product_with_nested_ctors(sub_patterns, field_types, raw_var, body, 2)
        } else {
            // Use simpler approach for vars and wildcards
            self.bind_product_patterns(sub_patterns, field_types, raw_var)?;
            let body_term = self.infer(body)?.0;
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
    fn build_left_nested_accessor(raw_var: &str, field_idx: usize, num_fields: usize) -> Term {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Ident, Path, Pattern};
    use tungsten_core::Context;

    /// Create an Elaborator for testing.
    fn make_elaborator() -> Elaborator<'static> {
        let ctx = Box::leak(Box::new(Context::new()));
        Elaborator::new(ctx)
    }

    /// Create a simple path from a string.
    fn simple_path(name: &str, span: crate::span::Span) -> Path {
        Path {
            segments: vec![Ident::new(name, span)],
            span,
        }
    }

    // ========================================================================
    // Tests for with_scoped_binding helper
    // ========================================================================

    /// Test that with_scoped_binding properly manages depth.
    #[test]
    fn test_with_scoped_binding_depth_management() {
        let mut elab = make_elaborator();
        let initial_depth = elab.depth;

        let result: ElabResult<usize> = elab.with_scoped_binding("x", Type::Nat, |e| {
            // Inside the closure, depth should be incremented
            Ok(e.depth)
        });

        assert!(result.is_ok());
        let inner_depth = result.unwrap();
        assert_eq!(inner_depth, initial_depth + 1);

        // After returning, depth should be restored
        assert_eq!(elab.depth, initial_depth);
    }

    /// Test that with_scoped_binding handles errors correctly.
    #[test]
    fn test_with_scoped_binding_error_handling() {
        use crate::elaborate::error::{ElabError, ElabErrorKind};
        use crate::span::Span;

        let mut elab = make_elaborator();
        let initial_depth = elab.depth;

        let result: ElabResult<()> = elab.with_scoped_binding("x", Type::Nat, |_| {
            Err(ElabError::new(
                Span::new(0, 0),
                ElabErrorKind::Other("test error".to_string()),
            ))
        });

        assert!(result.is_err());

        // Depth should still be restored even on error
        assert_eq!(elab.depth, initial_depth);
    }

    // ========================================================================
    // Tests for validate_ctor_arm
    // ========================================================================

    /// Test validate_ctor_arm passes with correct arity and no guard.
    #[test]
    fn test_validate_ctor_arm_success() {
        use crate::span::Span;

        let elab = make_elaborator();
        let span = Span::new(0, 0);

        let constructor = elab_env::Constructor {
            name: "Some".to_string(),
            fields: vec![Type::Nat],
            index: 0,
            span,
        };

        let arm = ast::MatchArm {
            pattern: Pattern::Constructor(
                simple_path("Some", span),
                vec![Pattern::Wildcard(span)],
                span,
            ),
            guard: None,
            body: ast::Expr::Unit(span),
            span,
        };

        let result = elab.validate_ctor_arm(&arm, &constructor, 1);
        assert!(result.is_ok());
    }

    /// Test validate_ctor_arm fails with guard.
    #[test]
    fn test_validate_ctor_arm_rejects_guard() {
        use crate::span::Span;

        let elab = make_elaborator();
        let span = Span::new(0, 0);

        let constructor = elab_env::Constructor {
            name: "Some".to_string(),
            fields: vec![Type::Nat],
            index: 0,
            span,
        };

        let arm = ast::MatchArm {
            pattern: Pattern::Constructor(
                simple_path("Some", span),
                vec![Pattern::Wildcard(span)],
                span,
            ),
            guard: Some(ast::Expr::BoolLiteral(true, span)),
            body: ast::Expr::Unit(span),
            span,
        };

        let result = elab.validate_ctor_arm(&arm, &constructor, 1);
        assert!(result.is_err());
    }

    /// Test validate_ctor_arm fails with wrong arity.
    #[test]
    fn test_validate_ctor_arm_wrong_arity() {
        use crate::span::Span;

        let elab = make_elaborator();
        let span = Span::new(0, 0);

        let constructor = elab_env::Constructor {
            name: "Some".to_string(),
            fields: vec![Type::Nat],
            index: 0,
            span,
        };

        let arm = ast::MatchArm {
            pattern: Pattern::Constructor(
                simple_path("Some", span),
                vec![], // 0 patterns but constructor expects 1
                span,
            ),
            guard: None,
            body: ast::Expr::Unit(span),
            span,
        };

        let result = elab.validate_ctor_arm(&arm, &constructor, 0);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tests for product field accessors (wrap_product_destructs)
    // ========================================================================

    /// Test wrap_product_destructs generates correct accessors for 2 fields.
    #[test]
    fn test_wrap_product_destructs_two_fields() {
        use crate::span::Span;

        let mut elab = make_elaborator();
        let span = Span::new(0, 0);

        // Simulate binding 2 patterns
        elab.depth = 2;

        let patterns = vec![
            Pattern::Var(Ident::new("a", span)),
            Pattern::Var(Ident::new("b", span)),
        ];
        let field_types = vec![Type::Nat, Type::String];
        let body = Term::var("result");

        let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
        assert!(result.is_ok());

        let term = result.unwrap();
        // Should generate: let a = fst(raw); let b = snd(raw); result
        // In reverse order: let b = ...; let a = ...; result
        // So outer is `let a = ...`
        if let Term::Let(var, ty, val, inner) = &term {
            assert_eq!(var.as_str(), "a");
            assert_eq!(ty, &Type::Nat);
            // val should be fst(raw)
            if let Term::Fst(inner_val) = val.as_ref() {
                if let Term::Var(v) = inner_val.as_ref() {
                    assert_eq!(v.as_str(), "raw");
                } else {
                    panic!("Expected Var in fst");
                }
            } else {
                panic!("Expected Fst for first field");
            }

            // Check inner let for b
            if let Term::Let(var2, ty2, val2, _) = inner.as_ref() {
                assert_eq!(var2.as_str(), "b");
                assert_eq!(ty2, &Type::String);
                // val2 should be snd(raw)
                if let Term::Snd(inner_val2) = val2.as_ref() {
                    if let Term::Var(v2) = inner_val2.as_ref() {
                        assert_eq!(v2.as_str(), "raw");
                    }
                } else {
                    panic!("Expected Snd for second field");
                }
            }
        } else {
            panic!("Expected Let term");
        }
    }

    /// Test wrap_product_destructs handles wildcards (skips them).
    #[test]
    fn test_wrap_product_destructs_with_wildcards() {
        use crate::span::Span;

        let mut elab = make_elaborator();
        let span = Span::new(0, 0);

        // Simulate binding 2 patterns
        elab.depth = 2;

        let patterns = vec![
            Pattern::Wildcard(span), // Should be skipped
            Pattern::Var(Ident::new("b", span)),
        ];
        let field_types = vec![Type::Nat, Type::String];
        let body = Term::var("result");

        let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
        assert!(result.is_ok());

        let term = result.unwrap();
        // Should only have one let for "b"
        if let Term::Let(var, _, _, _) = &term {
            assert_eq!(var.as_str(), "b");
        } else {
            panic!("Expected single Let term for non-wildcard");
        }
    }

    /// Test wrap_product_destructs generates correct accessors for 3 fields.
    #[test]
    fn test_wrap_product_destructs_three_fields() {
        use crate::span::Span;

        let mut elab = make_elaborator();
        let span = Span::new(0, 0);

        // Simulate binding 3 patterns
        elab.depth = 3;

        let patterns = vec![
            Pattern::Var(Ident::new("a", span)),
            Pattern::Var(Ident::new("b", span)),
            Pattern::Var(Ident::new("c", span)),
        ];
        let field_types = vec![Type::Nat, Type::String, Type::Bool];
        let body = Term::var("result");

        let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
        assert!(result.is_ok());

        // The structure should be:
        // let a = fst(raw); let b = fst(snd(raw)); let c = snd(snd(raw)); result
        // Depth should be decremented by 3
        assert_eq!(elab.depth, 0);
    }
}
