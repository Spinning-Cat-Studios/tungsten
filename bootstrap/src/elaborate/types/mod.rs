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
mod normalize;
mod paths;

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

            TypeExpr::Arrow(param, ret, _span) => {
                let param_ty = self.elab_type(param)?;
                let ret_ty = self.elab_type(ret)?;
                Ok(Type::arrow(param_ty, ret_ty))
            }

            TypeExpr::Product(left, right, _span) => {
                let left_ty = self.elab_type(left)?;
                let right_ty = self.elab_type(right)?;
                Ok(Type::product(left_ty, right_ty))
            }

            TypeExpr::Sum(left, right, _span) => {
                let left_ty = self.elab_type(left)?;
                let right_ty = self.elab_type(right)?;
                Ok(Type::sum(left_ty, right_ty))
            }

            TypeExpr::Forall(param, body, _span) => {
                // Enter type variable scope
                self.env.push_type_var(param.name.clone());
                let body_ty = self.elab_type(body)?;
                self.env.pop_type_var();

                Ok(Type::forall(&param.name, body_ty))
            }

            TypeExpr::App(base, args, span) => self.elab_type_app(base, args, *span),

            TypeExpr::Prop(_span) => Ok(Type::Prop),
            TypeExpr::Unit(_span) => Ok(Type::Unit),
            TypeExpr::Void(_span) => Ok(Type::Void),

            TypeExpr::Eq(left, right, span) => {
                // Equality type: a == b
                // We need to infer the type of the terms
                // This requires the terms to have the same type
                let (left_term, left_ty) = self.infer(left)?;
                let (right_term, right_ty) = self.infer(right)?;

                // Check that types match using α-equivalence
                if !self.types_equal(&left_ty, &right_ty) {
                    return Err(ElabError::type_mismatch(*span, left_ty, right_ty)
                        .with_note("both sides of equality must have the same type"));
                }

                Ok(Type::eq(left_ty, left_term, right_term))
            }

            TypeExpr::Paren(inner, _span) => self.elab_type(inner),

            TypeExpr::Ptr(inner, _span) => {
                let inner_ty = self.elab_type(inner)?;
                Ok(Type::ptr(inner_ty))
            }

            TypeExpr::Ref(inner, _span) => {
                let inner_ty = self.elab_type(inner)?;
                Ok(Type::ref_ty(inner_ty))
            }

            TypeExpr::Error(_span) => Err(ElabError::new(
                ty.span(),
                ElabErrorKind::Other("cannot elaborate error type".to_string()),
            )),
        }
    }
}
