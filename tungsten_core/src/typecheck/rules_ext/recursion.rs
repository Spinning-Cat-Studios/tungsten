//! Typing rules for equality elimination, fixpoints, and recursive types.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{check_type_wf, type_of, types_equal};
use crate::typecheck::TypeResult;
/// Type check substitution (equality elimination)
pub(in crate::typecheck) fn type_of_subst(
    ctx: &Context,
    ty: &Type,
    motive: &Type,
    eq_proof: &Term,
    proof: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    check_type_wf(ctx, motive)?;

    // Check motive P : τ → Prop
    match motive {
        Type::Arrow(from, to) if types_equal(from, ty) && **to == Type::Prop => {}
        _ => {
            return Err(TypeError::MotiveNotFunction {
                got: motive.clone(),
            });
        }
    }

    // Check eq_proof : Eq τ a b
    let eq_ty = type_of(ctx, eq_proof)?;
    let (_a, _b) = match &eq_ty {
        Type::Eq(eq_base_ty, a, b) if types_equal(eq_base_ty, ty) => (a, b),
        _ => return Err(TypeError::NotEquality { got: eq_ty }),
    };

    // Check proof : Prop (simplified - should be P a)
    let proof_ty = type_of(ctx, proof)?;
    if proof_ty != Type::Prop {
        return Err(TypeError::TypeMismatch {
            expected: Type::Prop,
            got: proof_ty,
        });
    }

    // Result is P b, which is Prop
    Ok(Type::Prop)
}

/// Type check fix point: `fix f:τ. t : τ`
pub(in crate::typecheck) fn type_of_fix(
    ctx: &Context,
    f: &str,
    ty: &Type,
    body: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    let body_ctx = ctx.with_term(f, ty.clone());
    let body_ty = type_of(&body_ctx, body)?;
    if !types_equal(&body_ty, ty) {
        return Err(TypeError::TypeMismatch {
            expected: ty.clone(),
            got: body_ty,
        });
    }
    Ok(ty.clone())
}

/// Type check fold: `fold [μα.τ] t : μα.τ`
pub(in crate::typecheck) fn type_of_fold(
    ctx: &Context,
    mu_ty: &Type,
    t: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, mu_ty)?;
    match mu_ty {
        Type::Mu(alpha, body) => {
            // Expected type for t is body with α replaced by μα.body
            let expected = body.substitute(alpha, mu_ty);
            let t_ty = type_of(ctx, t)?;
            if !types_equal(&t_ty, &expected) {
                return Err(TypeError::TypeMismatch {
                    expected,
                    got: t_ty,
                });
            }
            Ok(mu_ty.clone())
        }
        _ => Err(TypeError::MalformedType(mu_ty.clone())),
    }
}

/// Type check unfold: `unfold [μα.τ] t : τ[α := μα.τ]`
pub(in crate::typecheck) fn type_of_unfold(
    ctx: &Context,
    mu_ty: &Type,
    t: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, mu_ty)?;
    match mu_ty {
        Type::Mu(alpha, body) => {
            let t_ty = type_of(ctx, t)?;
            if !types_equal(&t_ty, mu_ty) {
                return Err(TypeError::TypeMismatch {
                    expected: mu_ty.clone(),
                    got: t_ty,
                });
            }
            Ok(body.substitute(alpha, mu_ty))
        }
        _ => Err(TypeError::MalformedType(mu_ty.clone())),
    }
}
