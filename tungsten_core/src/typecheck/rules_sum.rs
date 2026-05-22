//! Typing rules for sum types: Inl, Inr, Case.
//!
//! Split from `rules_core.rs` to keep per-file function counts manageable.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use super::error::TypeError;
use super::rules::{check_type_wf, type_of, types_equal};
use super::TypeResult;

/// A case branch: binding variable name and body term.
pub(super) struct CaseArm<'a> {
    pub var: &'a str,
    pub body: &'a Term,
}

/// Type check inl: `inl [τ₁ + τ₂] t : τ₁ + τ₂`
pub(super) fn type_of_inl(ctx: &Context, sum_ty: &Type, t: &Term) -> TypeResult<Type> {
    check_type_wf(ctx, sum_ty)?;
    match sum_ty {
        Type::Sum(ty1, _) => {
            let t_ty = type_of(ctx, t)?;
            if types_equal(&t_ty, ty1) {
                Ok(sum_ty.clone())
            } else {
                Err(TypeError::TypeMismatch {
                    expected: *ty1.clone(),
                    got: t_ty,
                })
            }
        }
        _ => Err(TypeError::NotASum {
            got: sum_ty.clone(),
        }),
    }
}

/// Type check inr: `inr [τ₁ + τ₂] t : τ₁ + τ₂`
pub(super) fn type_of_inr(ctx: &Context, sum_ty: &Type, t: &Term) -> TypeResult<Type> {
    check_type_wf(ctx, sum_ty)?;
    match sum_ty {
        Type::Sum(_, ty2) => {
            let t_ty = type_of(ctx, t)?;
            if types_equal(&t_ty, ty2) {
                Ok(sum_ty.clone())
            } else {
                Err(TypeError::TypeMismatch {
                    expected: *ty2.clone(),
                    got: t_ty,
                })
            }
        }
        _ => Err(TypeError::NotASum {
            got: sum_ty.clone(),
        }),
    }
}

/// Type check case: `case t of inl x => t₁ | inr y => t₂ : τ`
pub(super) fn type_of_case(
    ctx: &Context,
    scrut: &Term,
    left: &CaseArm<'_>,
    right: &CaseArm<'_>,
) -> TypeResult<Type> {
    let scrut_ty = type_of(ctx, scrut)?;
    match scrut_ty {
        Type::Sum(ty1, ty2) => {
            let left_ctx = ctx.with_term(left.var, *ty1);
            let left_ty = type_of(&left_ctx, left.body)?;

            let right_ctx = ctx.with_term(right.var, *ty2);
            let right_ty = type_of(&right_ctx, right.body)?;

            if types_equal(&left_ty, &right_ty) {
                Ok(left_ty)
            } else {
                Err(TypeError::BranchTypeMismatch {
                    then_type: left_ty,
                    else_type: right_ty,
                })
            }
        }
        _ => Err(TypeError::NotASum { got: scrut_ty }),
    }
}
