//! Typing rules for nat arithmetic, comparisons, and boolean operations,
//! and for the signed `Int` operations and bridges (ADR 14.9.26c).

use crate::context::Context;
use crate::terms::{IntBinOp, Term};
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{type_of, types_equal};
use crate::typecheck::TypeResult;

/// Check that `t` has exactly the primitive type `expected`.
fn expect_primitive(ctx: &Context, t: &Term, expected: &Type) -> TypeResult<()> {
    let got = type_of(ctx, t)?;
    if types_equal(&got, expected) {
        Ok(())
    } else {
        Err(TypeError::TypeMismatch {
            expected: expected.clone(),
            got,
        })
    }
}

/// Type check a signed binary operation: both operands `Int`; the result is
/// `Bool` for a comparison and `Int` otherwise.
pub(in crate::typecheck) fn type_of_int_bin(
    ctx: &Context,
    op: IntBinOp,
    t1: &Term,
    t2: &Term,
) -> TypeResult<Type> {
    expect_primitive(ctx, t1, &Type::Int)?;
    expect_primitive(ctx, t2, &Type::Int)?;
    Ok(if op.is_comparison() {
        Type::Bool
    } else {
        Type::Int
    })
}

/// Type check signed negation: `−t : Int`
pub(in crate::typecheck) fn type_of_int_neg(ctx: &Context, t: &Term) -> TypeResult<Type> {
    expect_primitive(ctx, t, &Type::Int)?;
    Ok(Type::Int)
}

/// Type check the `Nat → Int` bridge: `to_int(t) : Int`
pub(in crate::typecheck) fn type_of_nat_to_int(ctx: &Context, t: &Term) -> TypeResult<Type> {
    expect_primitive(ctx, t, &Type::Nat)?;
    Ok(Type::Int)
}

/// Type check the `Int → Nat` bridge: `from_int(t) : Nat`
pub(in crate::typecheck) fn type_of_int_to_nat(ctx: &Context, t: &Term) -> TypeResult<Type> {
    expect_primitive(ctx, t, &Type::Int)?;
    Ok(Type::Nat)
}
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
