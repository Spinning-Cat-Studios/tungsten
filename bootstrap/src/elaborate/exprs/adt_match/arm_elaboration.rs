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

use super::context::AdtIdentity;
use crate::elaborate::env::{self as elab_env};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Shared context for elaborating a single field's pattern within a constructor arm.
/// Bundles the params that flow through all dispatch branches.
struct FieldElabCtx<'a> {
    field_type: &'a Type,
    raw_var: &'a str,
    body: &'a ast::Expr,
    result_ty: Option<&'a Type>,
    depth: usize,
}

impl<'a> Elaborator<'a> {
    /// Elaborate a match arm - either a constructor pattern or a catch-all (wildcard/variable).
    pub(super) fn elab_ctor_arm_or_catch_all(
        &mut self,
        arm: &ast::MatchArm,
        ctor_ty: &Type, // The type of the value at this position in the sum
        constructor: &elab_env::Constructor,
        adt: &AdtIdentity<'_>,
        result_ty: Option<&Type>, // Expected result type (for nullary constructor check mode)
    ) -> ElabResult<(String, Term)> {
        match &arm.pattern {
            Pattern::Constructor(_, _, _) => {
                self.elab_ctor_arm(arm, ctor_ty, constructor, adt, result_ty)
            }
            Pattern::Wildcard(_) => {
                self.elab_wildcard_arm(&arm.body, ctor_ty, &constructor.name, result_ty)
            }
            Pattern::Var(ref var) => {
                // Check if this is actually a nullary constructor (parser can't distinguish)
                if constructor.name == var.name && constructor.fields.is_empty() {
                    // Nullary constructor written without parens (e.g., `Zero` not `Zero()`)
                    // Don't bind as variable — elaborate as constructor arm with no fields
                    let raw_var = format!("__ctor_{}", constructor.name);
                    self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
                        let body_term =
                            elab.elab_ctor_body(&[], &[], &raw_var, &arm.body, result_ty)?;
                        Ok((raw_var.clone(), body_term))
                    })
                } else {
                    self.elab_var_arm(&arm.body, ctor_ty, &var.name, result_ty)
                }
            }
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
        result_ty: Option<&Type>,
    ) -> ElabResult<(String, Term)> {
        let raw_var = format!("__catch_all_{}", ctor_name);
        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            let body_term = if let Some(expected) = result_ty {
                elab.check(body, expected)?
            } else {
                elab.infer(body)?.0
            };
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
        result_ty: Option<&Type>,
    ) -> ElabResult<(String, Term)> {
        let raw_var = var_name.to_string();
        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            let body_term = if let Some(expected) = result_ty {
                elab.check(body, expected)?
            } else {
                elab.infer(body)?.0
            };
            Ok((raw_var.clone(), body_term))
        })
    }

    /// Execute a closure with a scoped variable binding.
    /// Handles push/pop scope and depth management.
    pub(crate) fn with_scoped_binding<T, F>(&mut self, name: &str, ty: Type, f: F) -> ElabResult<T>
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
        adt: &AdtIdentity<'_>,
        result_ty: Option<&Type>, // Expected result type (for nullary constructor check mode)
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
            adt.type_params,
            adt.adt_type,
            adt.adt_name,
        );

        // Create a fresh variable for the raw matched value
        let raw_var = format!("__ctor_{}", constructor.name);

        self.with_scoped_binding(&raw_var, ctor_ty.clone(), |elab| {
            // Elaborate body with pattern bindings
            let body_term =
                elab.elab_ctor_body(sub_patterns, &field_types, &raw_var, &arm.body, result_ty)?;
            Ok((raw_var.clone(), body_term))
        })
    }

    /// Validate a constructor arm: check for guards and arity.
    pub(crate) fn validate_ctor_arm(
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
    ///
    /// For nullary constructors (no fields), uses `check` mode against `result_ty`
    /// when available. This prevents TyVar leaks: nullary arms don't bind any type
    /// variables through field destructuring, so without checking, unsubstituted
    /// TyVars can persist into Core IR and cause codegen size mismatches.
    fn elab_ctor_body(
        &mut self,
        sub_patterns: &[Pattern],
        field_types: &[Type],
        raw_var: &str,
        body: &ast::Expr,
        result_ty: Option<&Type>,
    ) -> ElabResult<Term> {
        if sub_patterns.is_empty() {
            // Nullary constructor: use check mode if we have an expected result type,
            // to prevent TyVar leaks from unsubstituted type parameters.
            if let Some(expected) = result_ty {
                self.check(body, expected)
            } else {
                self.infer(body).map(|(term, _)| term)
            }
        } else if sub_patterns.len() == 1 {
            // Single field: use check mode when result_ty is available to prevent
            // unconstrained type params from defaulting to Unit (ADR 15.5.26e).
            self.elab_single_field_pattern(
                &sub_patterns[0],
                &field_types[0],
                raw_var,
                body,
                result_ty,
            )
        } else {
            // Multiple fields: use check mode when result_ty is available to prevent
            // unconstrained type params from defaulting to Unit (ADR 15.5.26e).
            self.elab_multi_field_patterns(sub_patterns, field_types, raw_var, body, result_ty)
        }
    }

    /// Elaborate a single-field pattern.
    fn elab_single_field_pattern(
        &mut self,
        pattern: &Pattern,
        field_type: &Type,
        raw_var: &str,
        body: &ast::Expr,
        result_ty: Option<&Type>,
    ) -> ElabResult<Term> {
        let ctx = FieldElabCtx {
            field_type,
            raw_var,
            body,
            result_ty,
            depth: 2,
        };
        match pattern {
            Pattern::Wildcard(_) => {
                // Wildcard: use check mode when result_ty is available
                if let Some(expected) = ctx.result_ty {
                    self.check(ctx.body, expected)
                } else {
                    self.infer(ctx.body).map(|(term, _)| term)
                }
            }
            Pattern::Var(ref var) => self.elab_single_var_binding(&var.name, &ctx),
            Pattern::Constructor(_, _, _) => {
                // Nested constructor pattern: use recursive elaboration
                self.elab_nested_ctor_pattern(
                    pattern,
                    ctx.raw_var,
                    ctx.field_type,
                    ctx.body,
                    ctx.depth,
                )
            }
            Pattern::Tuple(ref sub_pats, tup_span) => {
                // Tuple pattern inside constructor (ADR 15.5.26f)
                self.elab_tuple_in_ctor_pattern(sub_pats, *tup_span, &ctx)
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
        ctx: &FieldElabCtx<'_>,
    ) -> ElabResult<Term> {
        self.env
            .bind_local(var_name.to_string(), ctx.field_type.clone(), self.depth);
        self.depth += 1;

        let body_term = if let Some(expected) = ctx.result_ty {
            self.check(ctx.body, expected)?
        } else {
            self.infer(ctx.body)?.0
        };
        let wrapped = Term::let_in(
            var_name,
            ctx.field_type.clone(),
            Term::var(ctx.raw_var),
            body_term,
        );

        self.depth -= 1;
        Ok(wrapped)
    }

    /// Elaborate a tuple pattern inside a constructor field (ADR 15.5.26f).
    ///
    /// For `Ok((a, b))`: the constructor has one field of tuple type, and
    /// the tuple sub-patterns destructure that field. Projection uses the
    /// tuple's right-nested convention (not the constructor's left-nested).
    fn elab_tuple_in_ctor_pattern(
        &mut self,
        sub_pats: &[Pattern],
        span: crate::span::Span,
        ctx: &FieldElabCtx<'_>,
    ) -> ElabResult<Term> {
        // Check depth limit at this level (consistent with elab_nested_ctor_pattern)
        if ctx.depth > crate::config::MAX_PATTERN_DEPTH {
            return Err(ElabError::new(
                span,
                ElabErrorKind::PatternTooDeep {
                    depth: ctx.depth,
                    max: crate::config::MAX_PATTERN_DEPTH,
                },
            ));
        }

        // 1. Extract tuple element types from the field type
        let elem_types = self.extract_tuple_types(ctx.field_type, sub_pats.len(), span)?;

        // 2. Collect bindings (left-to-right, depth-first) and register in env
        let mut bindings = Vec::new();
        for (pat, ty) in sub_pats.iter().zip(elem_types.iter()) {
            self.collect_bindings_from_pattern(pat, ty, &mut bindings, span)?;
        }
        for (name, ty) in &bindings {
            self.env.bind_local(name.clone(), ty.clone(), self.depth);
            self.depth += 1;
        }

        // 3. Elaborate body (check or infer based on result_ty)
        let body_term = if let Some(expected) = ctx.result_ty {
            self.check(ctx.body, expected)?
        } else {
            self.infer(ctx.body)?.0
        };

        // 4. Wrap with tuple projection lets
        let wrapped = self.build_tuple_lets(sub_pats, &elem_types, ctx.raw_var, body_term, span)?;

        // 5. Pop depth for all bindings
        for _ in &bindings {
            self.depth -= 1;
        }

        Ok(wrapped)
    }
}
