//! Built-in function elaboration.
//!
//! Handles:
//! - Ref cell operations (ref, get, set)
//! - String operations (char_at, string_len, substring)

use crate::ast::Expr;
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    // ═══════════════════════════════════════════════════════════════════════
    // Phase 3-Prep: Ref cell operations
    // ═══════════════════════════════════════════════════════════════════════

    /// Elaborate `ref(v)` - create a new ref cell
    pub(in crate::elaborate::exprs) fn elab_ref_new(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 1 {
            return Err(ElabError::arity_mismatch(span, 1, args.len())
                .with_help("`ref` takes exactly one argument: ref(value)"));
        }
        let (value_term, value_ty) = self.infer(&args[0])?;
        Ok((Term::ref_new(value_term), Type::ref_ty(value_ty)))
    }

    /// Elaborate `get(r)` - read from a ref cell
    pub(in crate::elaborate::exprs) fn elab_ref_get(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 1 {
            return Err(ElabError::arity_mismatch(span, 1, args.len())
                .with_help("`get` takes exactly one argument: get(ref)"));
        }
        let (ref_term, ref_ty) = self.infer(&args[0])?;

        // ref_ty must be Ref<T>
        let Type::Ref(inner_ty) = ref_ty else {
            return Err(ElabError::type_mismatch(
                args[0].span(),
                Type::ref_ty(Type::TyVar("T".into())),
                ref_ty,
            )
            .with_help("`get` expects a Ref<T>, not this type"));
        };

        Ok((Term::ref_get(ref_term), *inner_ty))
    }

    /// Elaborate `set(r, v)` - write to a ref cell
    pub(in crate::elaborate::exprs) fn elab_ref_set(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 2 {
            return Err(ElabError::arity_mismatch(span, 2, args.len())
                .with_help("`set` takes exactly two arguments: set(ref, value)"));
        }
        let (ref_term, ref_ty) = self.infer(&args[0])?;

        // ref_ty must be Ref<T>
        let Type::Ref(inner_ty) = ref_ty else {
            return Err(ElabError::type_mismatch(
                args[0].span(),
                Type::ref_ty(Type::TyVar("T".into())),
                ref_ty,
            )
            .with_help("`set` expects a Ref<T> as first argument"));
        };

        // Check value has type T
        let value_term = self.check(&args[1], &inner_ty)?;

        Ok((Term::ref_set(ref_term, value_term), Type::Unit))
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 3-Prep: String character access
    // ═══════════════════════════════════════════════════════════════════════

    /// Elaborate `char_at(s, n)` - get character at index
    pub(in crate::elaborate::exprs) fn elab_char_at(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 2 {
            return Err(ElabError::arity_mismatch(span, 2, args.len())
                .with_help("`char_at` takes exactly two arguments: char_at(string, index)"));
        }
        let str_term = self.check(&args[0], &Type::String)?;
        let idx_term = self.check(&args[1], &Type::Nat)?;

        Ok((Term::str_char_at(str_term, idx_term), Type::Nat))
    }

    /// Elaborate `string_len(s)` - get length of string
    pub(in crate::elaborate::exprs) fn elab_string_len(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 1 {
            return Err(ElabError::arity_mismatch(span, 1, args.len())
                .with_help("`string_len` takes exactly one argument: string_len(string)"));
        }
        let str_term = self.check(&args[0], &Type::String)?;

        Ok((Term::str_len(str_term), Type::Nat))
    }

    /// Elaborate `substring(s, start, len)` - get substring
    pub(in crate::elaborate::exprs) fn elab_substring(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        if args.len() != 3 {
            return Err(ElabError::arity_mismatch(span, 3, args.len()).with_help(
                "`substring` takes exactly three arguments: substring(string, start, length)",
            ));
        }
        let str_term = self.check(&args[0], &Type::String)?;
        let start_term = self.check(&args[1], &Type::Nat)?;
        let len_term = self.check(&args[2], &Type::Nat)?;

        Ok((
            Term::str_substring(str_term, start_term, len_term),
            Type::String,
        ))
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Test Assertions (ADR 4.5.26g, ADR 12.5.26c)
    // ═══════════════════════════════════════════════════════════════════════

    /// Elaborate `expect_type(expr, "TypeString")` — compile-time type assertion.
    pub(in crate::elaborate::exprs) fn elab_expect_type(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        use crate::elaborate::ElabMode;

        // In compile mode, reject entirely
        if self.elab_mode == ElabMode::Compile {
            return Err(ElabError::other(
                span,
                "expect_type is not allowed in compile/run mode (use `tungsten test` or `tungsten check`)",
            ));
        }

        // Validate arity = 2
        if args.len() != 2 {
            return Err(ElabError::other(
                span,
                &format!("expect_type: expected 2 arguments, found {}", args.len()),
            ));
        }

        // Validate second argument is a string literal
        let expected_str = match &args[1] {
            Expr::StringLiteral(s, _) => s.clone(),
            _ => {
                return Err(ElabError::other(
                    args[1].span(),
                    "expect_type: second argument must be a string literal",
                ));
            }
        };

        // Elaborate the first argument to get its type
        let (_term, inferred_ty) = self.infer(&args[0])?;

        // In test mode, compare type display string
        if self.elab_mode == ElabMode::Test {
            use crate::driver::output::format_type_for_display;
            let actual_str = format_type_for_display(&inferred_ty);
            if actual_str != expected_str {
                return Err(ElabError::other(
                    span,
                    &format!(
                        "type assertion failed: expected `{}`, found `{}`",
                        expected_str, actual_str
                    ),
                ));
            }
        }

        // Return Unit
        Ok((Term::Unit, Type::Unit))
    }

    /// Elaborate `expect_error(expr, "E0001")` — compile-time error assertion (ADR 12.5.26c).
    ///
    /// Elaborates the sub-expression in a snapshot of the current environment.
    /// If the expression produces an error with the expected code, the assertion passes.
    /// If the expression succeeds or produces a different error code, the assertion fails.
    pub(in crate::elaborate::exprs) fn elab_expect_error(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        use crate::elaborate::ElabMode;

        // In compile mode, reject entirely
        if self.elab_mode == ElabMode::Compile {
            return Err(ElabError::other(
                span,
                "expect_error is not allowed in compile/run mode (use `tungsten test` or `tungsten check`)",
            ));
        }

        // Validate arity = 2
        if args.len() != 2 {
            return Err(ElabError::other(
                span,
                &format!("expect_error: expected 2 arguments, found {}", args.len()),
            ));
        }

        // Validate second argument is a string literal (error code)
        let expected_code = match &args[1] {
            Expr::StringLiteral(s, _) => s.clone(),
            _ => {
                return Err(ElabError::other(
                    args[1].span(),
                    "expect_error: second argument must be a string literal error code (e.g. \"E0001\")",
                ));
            }
        };

        // Snapshot elaborator state
        let saved_env = self.env.clone();
        let saved_errors = std::mem::take(&mut self.errors);
        let saved_depth = self.depth;
        let saved_name_counter = self.name_counter;
        let saved_context_stack = std::mem::take(&mut self.context_stack);

        // Elaborate the sub-expression in the snapshot
        let result = self.infer(&args[0]);

        // Capture accumulated errors and restore state
        let captured_errors = std::mem::replace(&mut self.errors, saved_errors);
        self.env = saved_env;
        self.depth = saved_depth;
        self.name_counter = saved_name_counter;
        self.context_stack = saved_context_stack;

        match result {
            Ok((_term, inferred_ty)) => {
                // Expression succeeded — that's a test failure
                use crate::driver::output::format_type_for_display;
                let ty_str = format_type_for_display(&inferred_ty);
                Err(ElabError::other(
                    span,
                    &format!(
                        "expect_error: expected error {}, but expression elaborated successfully\n  inferred type: {}",
                        expected_code, ty_str
                    ),
                ))
            }
            Err(direct_error) => {
                // Collect all errors: the direct error + any accumulated
                let mut all_errors = vec![direct_error];
                all_errors.extend(captured_errors);

                // Check if any error matches the expected code
                if all_errors.iter().any(|e| e.kind.code() == expected_code) {
                    Ok((Term::Unit, Type::Unit))
                } else {
                    let primary = &all_errors[0];
                    Err(ElabError::other(
                        span,
                        &format!(
                            "expect_error: expected error {}, but got {}\n  actual error: {}",
                            expected_code,
                            primary.kind.code(),
                            primary.message
                        ),
                    ))
                }
            }
        }
    }
}
