//! Core typing rules: Lambda, App, Let, If, NatRec, NatInd,
//! TyApp, Refl, Annot, Absurd, Succ.
//!
//! Sum type rules (Inl, Inr, Case) are in `rules_sum.rs`.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use super::error::TypeError;
use super::rules::{check_type_wf, type_of, types_equal};
use super::TypeResult;

// ============================================================================
// Lambda / Application / Let
// ============================================================================

/// Type check application: `Γ ⊢ t₁ t₂ : τ₂` where `Γ ⊢ t₁ : τ₁ → τ₂`
pub(super) fn type_of_app(ctx: &Context, t1: &Term, t2: &Term) -> TypeResult<Type> {
    let ty1 = type_of(ctx, t1)?;
    match ty1 {
        Type::Arrow(param_ty, return_ty) => {
            let arg_ty = type_of(ctx, t2)?;
            if types_equal(&param_ty, &arg_ty) {
                Ok(*return_ty)
            } else {
                Err(TypeError::ArgumentTypeMismatch {
                    expected: *param_ty,
                    got: arg_ty,
                })
            }
        }
        _ => Err(TypeError::NotAFunction { got: ty1 }),
    }
}

/// Type check let binding: `Γ ⊢ let x : τ₁ = t₁ in t₂ : τ₂`
pub(super) fn type_of_let(
    ctx: &Context,
    x: &str,
    ty: &Type,
    def: &Term,
    body: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    let def_ty = type_of(ctx, def)?;
    if !types_equal(ty, &def_ty) {
        return Err(TypeError::TypeMismatch {
            expected: ty.clone(),
            got: def_ty,
        });
    }
    let body_ctx = ctx.with_term(x, ty.clone());
    type_of(&body_ctx, body)
}

/// Type check if-then-else: branches must agree on type.
pub(super) fn type_of_if(
    ctx: &Context,
    cond: &Term,
    then_: &Term,
    else_: &Term,
) -> TypeResult<Type> {
    let cond_ty = type_of(ctx, cond)?;
    if cond_ty != Type::Bool {
        return Err(TypeError::ConditionNotBool { got: cond_ty });
    }
    let then_ty = type_of(ctx, then_)?;
    let else_ty = type_of(ctx, else_)?;
    if types_equal(&then_ty, &else_ty) {
        Ok(then_ty)
    } else {
        Err(TypeError::BranchTypeMismatch {
            then_type: then_ty,
            else_type: else_ty,
        })
    }
}

// ============================================================================
// Natural number recursion / induction
// ============================================================================

/// Type check natrec: `natrec [τ] zero_case succ_case n : τ`
pub(super) fn type_of_natrec(
    ctx: &Context,
    ty: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;

    let zero_ty = type_of(ctx, zero_case)?;
    if !types_equal(&zero_ty, ty) {
        return Err(TypeError::TypeMismatch {
            expected: ty.clone(),
            got: zero_ty,
        });
    }

    let expected_succ_ty = Type::arrow(Type::Nat, Type::arrow(ty.clone(), ty.clone()));
    let succ_ty = type_of(ctx, succ_case)?;
    if !types_equal(&succ_ty, &expected_succ_ty) {
        return Err(TypeError::NatRecSuccTypeMismatch {
            expected: expected_succ_ty,
            got: succ_ty,
        });
    }

    let n_ty = type_of(ctx, n)?;
    if n_ty != Type::Nat {
        return Err(TypeError::NotANat { got: n_ty });
    }

    Ok(ty.clone())
}

/// Type check natind: `natind [P] zero_case succ_case n : P n`
pub(super) fn type_of_natind(
    ctx: &Context,
    motive: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, motive)?;

    match motive {
        Type::Arrow(from, to) if **from == Type::Nat && **to == Type::Prop => {}
        _ => {
            return Err(TypeError::NatIndMotiveMismatch {
                expected: Type::arrow(Type::Nat, Type::Prop),
                got: motive.clone(),
            });
        }
    }

    let zero_ty = type_of(ctx, zero_case)?;
    if zero_ty != Type::Prop {
        return Err(TypeError::TypeMismatch {
            expected: Type::Prop,
            got: zero_ty,
        });
    }

    let succ_ty = type_of(ctx, succ_case)?;
    let expected_succ = Type::arrow(Type::Nat, Type::arrow(Type::Prop, Type::Prop));
    if !types_equal(&succ_ty, &expected_succ) {
        return Err(TypeError::NatIndMotiveMismatch {
            expected: expected_succ,
            got: succ_ty,
        });
    }

    let n_ty = type_of(ctx, n)?;
    if n_ty != Type::Nat {
        return Err(TypeError::NotANat { got: n_ty });
    }

    Ok(Type::Prop)
}

// ============================================================================
// Polymorphism / Equality / Annotation
// ============================================================================

/// Type check type application: `t [τ'] : τ[α := τ']`
pub(super) fn type_of_tyapp(ctx: &Context, t: &Term, ty: &Type) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    let t_ty = type_of(ctx, t)?;
    match t_ty {
        Type::Forall(alpha, body) => Ok(body.substitute(&alpha, ty)),
        _ => Err(TypeError::NotPolymorphic { got: t_ty }),
    }
}

/// Type check refl: `refl [τ] t : Eq τ t t`
pub(super) fn type_of_refl(ctx: &Context, ty: &Type, t: &Term) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    let t_ty = type_of(ctx, t)?;
    if !types_equal(&t_ty, ty) {
        return Err(TypeError::TypeMismatch {
            expected: ty.clone(),
            got: t_ty,
        });
    }
    Ok(Type::eq(ty.clone(), t.clone(), t.clone()))
}

/// Type check annotation: `(t : τ) : τ`. Special-cases `sorry`.
pub(super) fn type_of_annot(ctx: &Context, t: &Term, ty: &Type) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    if matches!(t, Term::Sorry) {
        return Ok(ty.clone());
    }
    let t_ty = type_of(ctx, t)?;
    if types_equal(&t_ty, ty) {
        Ok(ty.clone())
    } else {
        Err(TypeError::TypeMismatch {
            expected: ty.clone(),
            got: t_ty,
        })
    }
}

// ============================================================================
// Absurd / Succ / Fst / Snd
// ============================================================================

/// Type check absurd elimination: `Γ ⊢ absurd [τ] t : τ` where `Γ ⊢ t : Void`
pub(super) fn type_of_absurd(ctx: &Context, ty: &Type, t: &Term) -> TypeResult<Type> {
    check_type_wf(ctx, ty)?;
    let t_ty = type_of(ctx, t)?;
    if t_ty == Type::Void {
        Ok(ty.clone())
    } else {
        Err(TypeError::NotVoid { got: t_ty })
    }
}

/// Type check successor: `Γ ⊢ succ t : Nat` where `Γ ⊢ t : Nat`
pub(super) fn type_of_succ(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let ty = type_of(ctx, t)?;
    if ty == Type::Nat {
        Ok(Type::Nat)
    } else {
        Err(TypeError::NotANat { got: ty })
    }
}

/// Type check first projection: `Γ ⊢ fst t : τ₁` where `Γ ⊢ t : τ₁ × τ₂`
pub(super) fn type_of_fst(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let ty = type_of(ctx, t)?;
    match ty {
        Type::Product(ty1, _) => Ok(*ty1),
        _ => Err(TypeError::NotAProduct { got: ty }),
    }
}

/// Type check second projection: `Γ ⊢ snd t : τ₂` where `Γ ⊢ t : τ₁ × τ₂`
pub(super) fn type_of_snd(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let ty = type_of(ctx, t)?;
    match ty {
        Type::Product(_, ty2) => Ok(*ty2),
        _ => Err(TypeError::NotAProduct { got: ty }),
    }
}
