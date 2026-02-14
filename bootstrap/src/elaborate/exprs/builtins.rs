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
    pub(super) fn elab_ref_new(&mut self, args: &[Expr], span: Span) -> ElabResult<(Term, Type)> {
        if args.len() != 1 {
            return Err(ElabError::arity_mismatch(span, 1, args.len())
                .with_help("`ref` takes exactly one argument: ref(value)"));
        }
        let (value_term, value_ty) = self.infer(&args[0])?;
        Ok((Term::ref_new(value_term), Type::ref_ty(value_ty)))
    }

    /// Elaborate `get(r)` - read from a ref cell
    pub(super) fn elab_ref_get(&mut self, args: &[Expr], span: Span) -> ElabResult<(Term, Type)> {
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
    pub(super) fn elab_ref_set(&mut self, args: &[Expr], span: Span) -> ElabResult<(Term, Type)> {
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
    pub(super) fn elab_char_at(&mut self, args: &[Expr], span: Span) -> ElabResult<(Term, Type)> {
        if args.len() != 2 {
            return Err(ElabError::arity_mismatch(span, 2, args.len())
                .with_help("`char_at` takes exactly two arguments: char_at(string, index)"));
        }
        let str_term = self.check(&args[0], &Type::String)?;
        let idx_term = self.check(&args[1], &Type::Nat)?;

        Ok((Term::str_char_at(str_term, idx_term), Type::Nat))
    }

    /// Elaborate `string_len(s)` - get length of string
    pub(super) fn elab_string_len(
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
    pub(super) fn elab_substring(&mut self, args: &[Expr], span: Span) -> ElabResult<(Term, Type)> {
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
}
