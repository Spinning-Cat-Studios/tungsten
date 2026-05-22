//! Typing rules for nat arithmetic, comparisons, and boolean operations.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{type_of, types_equal};
use crate::typecheck::TypeResult;
/// Type check binary Nat→Nat operations: `t₁ op t₂ : Nat`
pub(in crate::typecheck) fn type_of_nat_binop(
    ctx: &Context,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    let ty2 = type_of(ctx, t2)?;
    if !types_equal(&ty1, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: ty1,
        });
    }
    if !types_equal(&ty2, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: ty2,
        });
    }
    Ok(Type::Nat)
}

/// Type check binary Nat→Bool comparisons: `t₁ cmp t₂ : Bool`
pub(in crate::typecheck) fn type_of_nat_cmp(
    ctx: &Context,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    let ty2 = type_of(ctx, t2)?;
    if !types_equal(&ty1, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: ty1,
        });
    }
    if !types_equal(&ty2, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: ty2,
        });
    }
    Ok(Type::Bool)
}

/// Type check binary Bool→Bool operations: `t₁ op t₂ : Bool`
pub(in crate::typecheck) fn type_of_bool_binop(
    ctx: &Context,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    let ty2 = type_of(ctx, t2)?;
    if !types_equal(&ty1, &Type::Bool) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Bool,
            got: ty1,
        });
    }
    if !types_equal(&ty2, &Type::Bool) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Bool,
            got: ty2,
        });
    }
    Ok(Type::Bool)
}

/// Type check boolean negation: `!t : Bool`
pub(in crate::typecheck) fn type_of_bool_not(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let ty = type_of(ctx, t)?;
    if !types_equal(&ty, &Type::Bool) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Bool,
            got: ty,
        });
    }
    Ok(Type::Bool)
}
