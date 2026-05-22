//! Typing rules implementation
//!
//! This module contains the main type synthesis function `type_of` which
//! implements the typing judgment Γ ⊢ t : τ.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use super::error::TypeError;
use super::TypeResult;

/// Check if a type is well-formed under a context
pub fn check_type_wf(ctx: &Context, ty: &Type) -> TypeResult<()> {
    let type_vars = ctx.type_vars();
    if ty.is_well_formed(&type_vars) {
        Ok(())
    } else {
        Err(TypeError::MalformedType(ty.clone()))
    }
}

/// Check if two types are definitionally equal
///
/// Uses α-equivalence to handle bound variable renaming in μ-types and ∀-types.
/// This ensures that `μα. Unit + α` equals `μβ. Unit + β`.
#[must_use]
pub fn types_equal(ty1: &Type, ty2: &Type) -> bool {
    // Use α-equivalent type equality from types module
    crate::types::types_equal_alpha(ty1, ty2)
}

/// Type check a term against an expected type
pub fn check(ctx: &Context, term: &Term, expected: &Type) -> TypeResult<()> {
    let inferred = type_of(ctx, term)?;
    if types_equal(&inferred, expected) {
        Ok(())
    } else {
        Err(TypeError::TypeMismatch {
            expected: expected.clone(),
            got: inferred,
        })
    }
}

/// Synthesize the type of a term: Γ ⊢ t : τ
pub fn type_of(ctx: &Context, term: &Term) -> TypeResult<Type> {
    match term {
        Term::Var(v) => ctx
            .lookup_term(v)
            .cloned()
            .ok_or_else(|| TypeError::UnboundVariable(v.clone())),
        Term::Global(name) => ctx
            .lookup_term(name)
            .cloned()
            .ok_or_else(|| TypeError::UnboundVariable(name.clone())),

        Term::Lambda(x, ty, body) => {
            check_type_wf(ctx, ty)?;
            let body_ctx = ctx.with_term(x, ty.clone());
            let body_ty = type_of(&body_ctx, body)?;
            Ok(Type::arrow(ty.clone(), body_ty))
        }
        Term::TyAbs(alpha, body) => {
            let body_ctx = ctx.with_type_var(alpha);
            let body_ty = type_of(&body_ctx, body)?;
            Ok(Type::forall(alpha.clone(), body_ty))
        }
        Term::Pair(t1, t2) => {
            let ty1 = type_of(ctx, t1)?;
            let ty2 = type_of(ctx, t2)?;
            Ok(Type::product(ty1, ty2))
        }
        Term::RefNew(t) => Ok(Type::ref_ty(type_of(ctx, t)?)),

        // Literal types
        Term::True | Term::False => Ok(Type::Bool),
        Term::Unit => Ok(Type::Unit),
        Term::Zero | Term::NatLit(_) => Ok(Type::Nat),
        Term::StringLit(_) => Ok(Type::String),
        Term::Sorry | Term::ExternCall(_, _) => Ok(Type::Unit),

        // Core language forms (delegated to rules_core)
        Term::App(t1, t2) => super::rules_core::type_of_app(ctx, t1, t2),
        Term::Let(x, ty, def, body) => super::rules_core::type_of_let(ctx, x, ty, def, body),
        Term::If(cond, then_, else_) => super::rules_core::type_of_if(ctx, cond, then_, else_),
        Term::Absurd(ty, t) => super::rules_core::type_of_absurd(ctx, ty, t),
        Term::Succ(t) => super::rules_core::type_of_succ(ctx, t),
        Term::NatRec(ty, z, s, n) => super::rules_core::type_of_natrec(ctx, ty, z, s, n),
        Term::NatInd(m, z, s, n) => super::rules_core::type_of_natind(ctx, m, z, s, n),
        Term::Fst(t) => super::rules_core::type_of_fst(ctx, t),
        Term::Snd(t) => super::rules_core::type_of_snd(ctx, t),
        Term::Inl(sum_ty, t) => super::rules_sum::type_of_inl(ctx, sum_ty, t),
        Term::Inr(sum_ty, t) => super::rules_sum::type_of_inr(ctx, sum_ty, t),
        Term::Case(scrut, x, l, y, r) => {
            use super::rules_sum::CaseArm;
            super::rules_sum::type_of_case(
                ctx,
                scrut,
                &CaseArm { var: x, body: l },
                &CaseArm { var: y, body: r },
            )
        }
        Term::TyApp(t, ty) => super::rules_core::type_of_tyapp(ctx, t, ty),
        Term::Refl(ty, t) => super::rules_core::type_of_refl(ctx, ty, t),
        Term::Annot(t, ty) => super::rules_core::type_of_annot(ctx, t, ty),

        // Extended features (delegated to rules_ext)
        Term::Subst(ty, m, eq, p) => super::rules_ext::type_of_subst(ctx, ty, m, eq, p),
        Term::Fix(f, ty, body) => super::rules_ext::type_of_fix(ctx, f, ty, body),
        Term::Fold(mu_ty, t) => super::rules_ext::type_of_fold(ctx, mu_ty, t),
        Term::Unfold(mu_ty, t) => super::rules_ext::type_of_unfold(ctx, mu_ty, t),

        // String operations
        Term::StrConcat(t1, t2) => super::rules_ext::type_of_str_concat(ctx, t1, t2),
        Term::StrLen(t) => super::rules_ext::type_of_str_len(ctx, t),
        Term::StrEq(t1, t2) => super::rules_ext::type_of_str_eq(ctx, t1, t2),
        Term::StrCharAt(s, n) => super::rules_ext::type_of_str_char_at(ctx, s, n),
        Term::StrSubstring(s, start, len) => {
            super::rules_ext::type_of_str_substring(ctx, s, start, len)
        }

        // Arithmetic, comparison, and boolean operations
        Term::NatAdd(t1, t2)
        | Term::NatSub(t1, t2)
        | Term::NatMul(t1, t2)
        | Term::NatDiv(t1, t2)
        | Term::NatMod(t1, t2) => super::rules_ext::type_of_nat_binop(ctx, t1, t2),
        Term::NatEq(t1, t2)
        | Term::NatLt(t1, t2)
        | Term::NatLe(t1, t2)
        | Term::NatGt(t1, t2)
        | Term::NatGe(t1, t2) => super::rules_ext::type_of_nat_cmp(ctx, t1, t2),
        Term::BoolAnd(t1, t2) | Term::BoolOr(t1, t2) => {
            super::rules_ext::type_of_bool_binop(ctx, t1, t2)
        }
        Term::BoolNot(t) => super::rules_ext::type_of_bool_not(ctx, t),

        // Ref cells
        Term::RefGet(t) => super::rules_ext::type_of_ref_get(ctx, t),
        Term::RefSet(r, v) => super::rules_ext::type_of_ref_set(ctx, r, v),

        // ADTs
        Term::AdtConstruct(adt_ty, idx, payload) => {
            super::rules_ext::type_of_adt_construct(ctx, adt_ty, *idx, payload)
        }
        Term::AdtMatch(scrut, arms) => super::rules_ext::type_of_adt_match(ctx, scrut, arms),

        // Span wrapper / control flow — transparent to typing
        Term::Spanned(inner, _) => type_of(ctx, inner),
        Term::Return(t) => super::rules_ext::type_of_return(ctx, t),
    }
}
