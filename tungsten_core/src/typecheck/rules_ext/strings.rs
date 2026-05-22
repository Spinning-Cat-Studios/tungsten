//! Typing rules for string operations.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{type_of, types_equal};
use crate::typecheck::TypeResult;
/// Type check string concatenation: `strconcat t₁ t₂ : String`
pub(in crate::typecheck) fn type_of_str_concat(
    ctx: &Context,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    let ty2 = type_of(ctx, t2)?;
    if !types_equal(&ty1, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: ty1,
        });
    }
    if !types_equal(&ty2, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: ty2,
        });
    }
    Ok(Type::String)
}

/// Type check string length: `strlen t : Nat`
pub(in crate::typecheck) fn type_of_str_len(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let ty = type_of(ctx, t)?;
    if !types_equal(&ty, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: ty,
        });
    }
    Ok(Type::Nat)
}

/// Type check string equality: `streq t₁ t₂ : Bool`
pub(in crate::typecheck) fn type_of_str_eq(
    ctx: &Context,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    let ty2 = type_of(ctx, t2)?;
    if !types_equal(&ty1, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: ty1,
        });
    }
    if !types_equal(&ty2, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: ty2,
        });
    }
    Ok(Type::Bool)
}

/// Type check string char_at: `char_at s n : Nat`
pub(in crate::typecheck) fn type_of_str_char_at(
    ctx: &Context,
    s: &Term,
    n: &Term,
) -> TypeResult<Type> {
    let s_ty = type_of(ctx, s)?;
    let n_ty = type_of(ctx, n)?;
    if !types_equal(&s_ty, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: s_ty,
        });
    }
    if !types_equal(&n_ty, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: n_ty,
        });
    }
    Ok(Type::Nat)
}

/// Type check substring: `substring s start len : String`
pub(in crate::typecheck) fn type_of_str_substring(
    ctx: &Context,
    s: &Term,
    start: &Term,
    len: &Term,
) -> TypeResult<Type> {
    let s_ty = type_of(ctx, s)?;
    let start_ty = type_of(ctx, start)?;
    let len_ty = type_of(ctx, len)?;
    if !types_equal(&s_ty, &Type::String) {
        return Err(TypeError::TypeMismatch {
            expected: Type::String,
            got: s_ty,
        });
    }
    if !types_equal(&start_ty, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: start_ty,
        });
    }
    if !types_equal(&len_ty, &Type::Nat) {
        return Err(TypeError::TypeMismatch {
            expected: Type::Nat,
            got: len_ty,
        });
    }
    Ok(Type::String)
}
