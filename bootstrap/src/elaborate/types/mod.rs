//! Type Elaboration
//!
//! This module elaborates surface syntax types (`ast::TypeExpr`) into
//! core calculus types (`tungsten_core::Type`).
//!
//! # Organization
//!
//! - `mod.rs` — Entry point (`elab_type`), re-exports
//! - `paths.rs` — Path and name resolution for types
//! - `encoding.rs` — ADT and record type encoding
//! - `normalize/` — Type normalization for structural comparison (see normalize/mod.rs)

mod encoding;
mod encoding_utils;
mod normalize;
mod paths;
pub(crate) mod resolve_refs;

#[cfg(test)]
mod tests;

use crate::ast::TypeExpr;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;
use crate::span::Spanned;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Elaborate a surface syntax type expression into a Core type.
    ///
    /// This is the main entry point for type elaboration.
    pub fn elab_type(&mut self, ty: &TypeExpr) -> ElabResult<Type> {
        match ty {
            TypeExpr::Path(path) => self.elab_type_path(path),

            TypeExpr::Arrow(a, b, _span)
            | TypeExpr::Product(a, b, _span)
            | TypeExpr::Sum(a, b, _span) => self.elab_binary_type(ty, a, b),

            TypeExpr::Forall(param, body, _span) => self.elab_forall_type(param, body),

            TypeExpr::App(base, args, span) => self.elab_type_app(base, args, *span),

            TypeExpr::Prop(_span) => Ok(Type::Prop),
            TypeExpr::Unit(_span) => Ok(Type::Unit),
            TypeExpr::Void(_span) => Ok(Type::Void),

            TypeExpr::Eq(left, right, span) => self.elab_type_eq(left, right, *span),

            TypeExpr::EqExplicit(ty, left, right, span) => {
                self.elab_type_eq_explicit(ty, left, right, *span)
            }

            TypeExpr::Paren(inner, _span) => self.elab_type(inner),
            TypeExpr::Ptr(inner, _span) | TypeExpr::Ref(inner, _span) => {
                let inner_ty = self.elab_type(inner)?;
                Ok(if matches!(ty, TypeExpr::Ptr(..)) {
                    Type::ptr(inner_ty)
                } else {
                    Type::ref_ty(inner_ty)
                })
            }

            TypeExpr::Error(_span) => Err(ElabError::new(
                ty.span(),
                ElabErrorKind::Other("cannot elaborate error type".to_string()),
            )),
        }
    }

    /// Elaborate a binary type constructor (Arrow, Product, Sum).
    fn elab_binary_type(&mut self, ty: &TypeExpr, a: &TypeExpr, b: &TypeExpr) -> ElabResult<Type> {
        let left = self.elab_type(a)?;
        let right = self.elab_type(b)?;
        Ok(match ty {
            TypeExpr::Arrow(..) => Type::arrow(left, right),
            TypeExpr::Product(..) => Type::product(left, right),
            _ => Type::sum(left, right),
        })
    }

    /// Elaborate a universally quantified type.
    fn elab_forall_type(&mut self, param: &crate::ast::Ident, body: &TypeExpr) -> ElabResult<Type> {
        self.env.push_type_var(param.name.clone());
        let body_ty = self.elab_type(body)?;
        self.env.pop_type_var();
        Ok(Type::forall(&param.name, body_ty))
    }

    /// Elaborate an equality type `Eq<left, right>`.
    fn elab_type_eq(
        &mut self,
        left: &crate::ast::Expr,
        right: &crate::ast::Expr,
        span: crate::span::Span,
    ) -> ElabResult<Type> {
        let (left_term, left_ty) = self.infer(left)?;
        let (right_term, right_ty) = self.infer(right)?;

        if !self.types_equal(&left_ty, &right_ty) {
            return Err(ElabError::type_mismatch(span, left_ty, right_ty)
                .with_note("both sides of equality must have the same type"));
        }

        Ok(Type::eq(left_ty, left_term, right_term))
    }

    /// Elaborate an explicit equality type `Eq<T, a, b>`.
    fn elab_type_eq_explicit(
        &mut self,
        ty: &TypeExpr,
        left: &crate::ast::Expr,
        right: &crate::ast::Expr,
        span: crate::span::Span,
    ) -> ElabResult<Type> {
        let base_ty = self.elab_type(ty)?;
        let (left_term, left_ty) = self.infer(left)?;
        let (right_term, right_ty) = self.infer(right)?;

        if !self.types_equal(&left_ty, &base_ty) {
            return Err(ElabError::type_mismatch(span, base_ty, left_ty)
                .with_note("left side of Eq does not match the declared type"));
        }

        if !self.types_equal(&right_ty, &base_ty) {
            return Err(ElabError::type_mismatch(span, base_ty, right_ty)
                .with_note("right side of Eq does not match the declared type"));
        }

        Ok(Type::eq(base_ty, left_term, right_term))
    }
}
