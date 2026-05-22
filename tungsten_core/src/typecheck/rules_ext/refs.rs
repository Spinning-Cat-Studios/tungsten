//! Typing rules for reference cells.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{type_of, types_equal};
use crate::typecheck::TypeResult;
/// Type check ref get: `get t : τ` where `t : Ref<τ>`
pub(in crate::typecheck) fn type_of_ref_get(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let t_ty = type_of(ctx, t)?;
    match t_ty {
        Type::Ref(inner) => Ok(*inner),
        _ => Err(TypeError::TypeMismatch {
            expected: Type::ref_ty(Type::TyVar("τ".into())),
            got: t_ty,
        }),
    }
}

/// Type check ref set: `set r v : Unit` where `r : Ref<τ>`, `v : τ`
pub(in crate::typecheck) fn type_of_ref_set(ctx: &Context, r: &Term, v: &Term) -> TypeResult<Type> {
    let r_ty = type_of(ctx, r)?;
    match r_ty {
        Type::Ref(inner) => {
            let v_ty = type_of(ctx, v)?;
            if !types_equal(&v_ty, &inner) {
                return Err(TypeError::TypeMismatch {
                    expected: *inner,
                    got: v_ty,
                });
            }
            Ok(Type::Unit)
        }
        _ => Err(TypeError::TypeMismatch {
            expected: Type::ref_ty(Type::TyVar("τ".into())),
            got: r_ty,
        }),
    }
}
