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
        // (VAR)
        // x : τ ∈ Γ
        // ──────────
        // Γ ⊢ x : τ
        Term::Var(v) => ctx
            .lookup_term(v)
            .cloned()
            .ok_or_else(|| TypeError::UnboundVariable(v.clone())),

        // (GLOBAL)
        // Global references are treated like variables during typechecking.
        // The elaborator ensures globals are in scope before emitting Term::Global.
        Term::Global(name) => ctx
            .lookup_term(name)
            .cloned()
            .ok_or_else(|| TypeError::UnboundVariable(name.clone())),

        // (ABS)
        // Γ, x:τ₁ ⊢ t : τ₂
        // ────────────────────────
        // Γ ⊢ λx:τ₁. t : τ₁ → τ₂
        Term::Lambda(x, ty, body) => {
            check_type_wf(ctx, ty)?;
            let body_ctx = ctx.with_term(x, ty.clone());
            let body_ty = type_of(&body_ctx, body)?;
            Ok(Type::arrow(ty.clone(), body_ty))
        }

        // (APP)
        // Γ ⊢ t₁ : τ₁ → τ₂   Γ ⊢ t₂ : τ₁
        // ───────────────────────────────
        // Γ ⊢ t₁ t₂ : τ₂
        Term::App(t1, t2) => {
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

        // (LET)
        // Γ ⊢ t₁ : τ₁
        // Γ, x:τ₁ ⊢ t₂ : τ₂
        // ─────────────────────────────────
        // Γ ⊢ let x : τ₁ = t₁ in t₂ : τ₂
        Term::Let(x, ty, def, body) => {
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

        // (TRUE) / (FALSE)
        Term::True | Term::False => Ok(Type::Bool),

        // (IF)
        // Γ ⊢ t_cond : Bool
        // Γ ⊢ t_then : τ
        // Γ ⊢ t_else : τ
        // ───────────────────────────────
        // Γ ⊢ if t_cond then t_then else t_else : τ
        Term::If(cond, then_, else_) => {
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

        // (UNIT)
        Term::Unit => Ok(Type::Unit),

        // (ABSURD)
        // Γ ⊢ t : Void      Γ ⊢ τ type
        // ───────────────────────────────
        // Γ ⊢ absurd [τ] t : τ
        Term::Absurd(ty, t) => {
            check_type_wf(ctx, ty)?;
            let t_ty = type_of(ctx, t)?;
            if t_ty == Type::Void {
                Ok(ty.clone())
            } else {
                Err(TypeError::NotVoid { got: t_ty })
            }
        }

        // (ZERO)
        Term::Zero => Ok(Type::Nat),

        // (NATLIT) - Efficient representation for large natural numbers
        Term::NatLit(_) => Ok(Type::Nat),

        // (SUCC)
        // Γ ⊢ t : Nat
        // ──────────────
        // Γ ⊢ succ t : Nat
        Term::Succ(t) => {
            let ty = type_of(ctx, t)?;
            if ty == Type::Nat {
                Ok(Type::Nat)
            } else {
                Err(TypeError::NotANat { got: ty })
            }
        }

        // (NATREC)
        // Γ ⊢ τ type
        // Γ ⊢ t_zero : τ
        // Γ ⊢ t_succ : Nat → τ → τ
        // Γ ⊢ t_n : Nat
        // ──────────────────────────────────────────
        // Γ ⊢ natrec [τ] t_zero t_succ t_n : τ
        Term::NatRec(ty, zero_case, succ_case, n) => {
            check_type_wf(ctx, ty)?;

            // Check zero case has type τ
            let zero_ty = type_of(ctx, zero_case)?;
            if !types_equal(&zero_ty, ty) {
                return Err(TypeError::TypeMismatch {
                    expected: ty.clone(),
                    got: zero_ty,
                });
            }

            // Check succ case has type Nat → τ → τ
            let expected_succ_ty = Type::arrow(Type::Nat, Type::arrow(ty.clone(), ty.clone()));
            let succ_ty = type_of(ctx, succ_case)?;
            if !types_equal(&succ_ty, &expected_succ_ty) {
                return Err(TypeError::NatRecSuccTypeMismatch {
                    expected: expected_succ_ty,
                    got: succ_ty,
                });
            }

            // Check n has type Nat
            let n_ty = type_of(ctx, n)?;
            if n_ty != Type::Nat {
                return Err(TypeError::NotANat { got: n_ty });
            }

            Ok(ty.clone())
        }

        // (NATIND)
        // Γ ⊢ P : Nat → Prop
        // Γ ⊢ p_zero : P zero
        // Γ ⊢ p_succ : ∀n:Nat. P n → P (succ n)
        // Γ ⊢ t_n : Nat
        // ──────────────────────────────────────────
        // Γ ⊢ natind [P] p_zero p_succ t_n : P t_n
        Term::NatInd(motive, zero_case, succ_case, n) => {
            check_type_wf(ctx, motive)?;

            // Check P : Nat → Prop
            match motive {
                Type::Arrow(from, to) if **from == Type::Nat && **to == Type::Prop => {}
                _ => {
                    return Err(TypeError::NatIndMotiveMismatch {
                        expected: Type::arrow(Type::Nat, Type::Prop),
                        got: motive.clone(),
                    });
                }
            }

            // For now, we do a simplified check:
            // - zero_case should have type that results from applying P to zero
            // - This is complex because P is a type and we'd need to substitute
            // In Phase 1 with quasi-dependent types, we check structurally

            // Check zero case: should be P applied to zero
            // Since P : Nat → Prop, P zero : Prop
            let zero_ty = type_of(ctx, zero_case)?;
            // We expect something of type Prop
            // (In a full dependent system we'd check P zero specifically)
            if zero_ty != Type::Prop {
                return Err(TypeError::TypeMismatch {
                    expected: Type::Prop,
                    got: zero_ty,
                });
            }

            // Check succ case: Nat → Prop → Prop
            // (Simplified: we expect the induction hypothesis)
            let succ_ty = type_of(ctx, succ_case)?;
            let expected_succ = Type::arrow(Type::Nat, Type::arrow(Type::Prop, Type::Prop));
            if !types_equal(&succ_ty, &expected_succ) {
                return Err(TypeError::NatIndMotiveMismatch {
                    expected: expected_succ,
                    got: succ_ty,
                });
            }

            // Check n : Nat
            let n_ty = type_of(ctx, n)?;
            if n_ty != Type::Nat {
                return Err(TypeError::NotANat { got: n_ty });
            }

            // Result type is Prop (P applied to n)
            Ok(Type::Prop)
        }

        // (PAIR)
        // Γ ⊢ t₁ : τ₁   Γ ⊢ t₂ : τ₂
        // ──────────────────────────
        // Γ ⊢ (t₁, t₂) : τ₁ × τ₂
        Term::Pair(t1, t2) => {
            let ty1 = type_of(ctx, t1)?;
            let ty2 = type_of(ctx, t2)?;
            Ok(Type::product(ty1, ty2))
        }

        // (FST)
        // Γ ⊢ t : τ₁ × τ₂
        // ────────────────
        // Γ ⊢ fst t : τ₁
        Term::Fst(t) => {
            let ty = type_of(ctx, t)?;
            match ty {
                Type::Product(ty1, _) => Ok(*ty1),
                _ => Err(TypeError::NotAProduct { got: ty }),
            }
        }

        // (SND)
        // Γ ⊢ t : τ₁ × τ₂
        // ────────────────
        // Γ ⊢ snd t : τ₂
        Term::Snd(t) => {
            let ty = type_of(ctx, t)?;
            match ty {
                Type::Product(_, ty2) => Ok(*ty2),
                _ => Err(TypeError::NotAProduct { got: ty }),
            }
        }

        // (INL)
        // Γ ⊢ t : τ₁      Γ ⊢ τ₂ type
        // ──────────────────────────────
        // Γ ⊢ inl [τ₁ + τ₂] t : τ₁ + τ₂
        Term::Inl(sum_ty, t) => {
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

        // (INR)
        // Γ ⊢ t : τ₂      Γ ⊢ τ₁ type
        // ──────────────────────────────
        // Γ ⊢ inr [τ₁ + τ₂] t : τ₁ + τ₂
        Term::Inr(sum_ty, t) => {
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

        // (CASE)
        // Γ ⊢ t : τ₁ + τ₂
        // Γ, x:τ₁ ⊢ t₁ : τ
        // Γ, y:τ₂ ⊢ t₂ : τ
        // ───────────────────────────────────────────────
        // Γ ⊢ case t of inl x => t₁ | inr y => t₂ : τ
        Term::Case(scrut, x, left, y, right) => {
            let scrut_ty = type_of(ctx, scrut)?;
            match scrut_ty {
                Type::Sum(ty1, ty2) => {
                    let left_ctx = ctx.with_term(x, *ty1);
                    let left_ty = type_of(&left_ctx, left)?;

                    let right_ctx = ctx.with_term(y, *ty2);
                    let right_ty = type_of(&right_ctx, right)?;

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

        // (TABS)
        // Γ, α ⊢ t : τ
        // ────────────────────
        // Γ ⊢ Λα. t : ∀α. τ
        Term::TyAbs(alpha, body) => {
            let body_ctx = ctx.with_type_var(alpha);
            let body_ty = type_of(&body_ctx, body)?;
            Ok(Type::forall(alpha.clone(), body_ty))
        }

        // (TAPP)
        // Γ ⊢ t : ∀α. τ      Γ ⊢ τ' type
        // ───────────────────────────────
        // Γ ⊢ t [τ'] : τ[α := τ']
        Term::TyApp(t, ty) => {
            check_type_wf(ctx, ty)?;
            let t_ty = type_of(ctx, t)?;
            match t_ty {
                Type::Forall(alpha, body) => Ok(body.substitute(&alpha, ty)),
                _ => Err(TypeError::NotPolymorphic { got: t_ty }),
            }
        }

        // (REFL)
        // Γ ⊢ t : τ
        // ─────────────────────────
        // Γ ⊢ refl [τ] t : Eq τ t t
        Term::Refl(ty, t) => {
            check_type_wf(ctx, ty)?;
            let t_ty = type_of(ctx, t)?;
            if !types_equal(&t_ty, ty) {
                return Err(TypeError::TypeMismatch {
                    expected: ty.clone(),
                    got: t_ty,
                });
            }
            Ok(Type::eq(ty.clone(), t.as_ref().clone(), t.as_ref().clone()))
        }

        // (SUBST)
        // Γ ⊢ P : τ → Prop
        // Γ ⊢ t_eq : Eq τ a b
        // Γ ⊢ t_proof : P a
        // ────────────────────────────────
        // Γ ⊢ subst [τ] [P] t_eq t_proof : P b
        Term::Subst(ty, motive, eq_proof, proof) => {
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

        // Type annotation
        // Special case: (sorry : τ) allows sorry to have any annotated type
        Term::Annot(t, ty) => {
            check_type_wf(ctx, ty)?;
            // If inner term is sorry, accept the annotation as-is
            if matches!(t.as_ref(), Term::Sorry) {
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

        // sorry - type-checks as any type, but we can't synthesize without annotation
        // In practice, sorry would need a type annotation: (sorry : τ)
        Term::Sorry => {
            // Without an annotation, we can't know what type sorry should have
            // Return Unit as a placeholder (in real use, sorry is always annotated)
            Ok(Type::Unit)
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: Strings
        // ═══════════════════════════════════════════════════════════════════

        // (STRINGLIT)
        // ─────────────────────
        // Γ ⊢ "s" : String
        Term::StringLit(_) => Ok(Type::String),

        // (STRCONCAT)
        // Γ ⊢ t₁ : String   Γ ⊢ t₂ : String
        // ──────────────────────────────────
        // Γ ⊢ strconcat t₁ t₂ : String
        Term::StrConcat(t1, t2) => {
            let ty1 = type_of(ctx, t1)?;
            let ty2 = type_of(ctx, t2)?;
            if ty1 != Type::String {
                return Err(TypeError::TypeMismatch {
                    expected: Type::String,
                    got: ty1,
                });
            }
            if ty2 != Type::String {
                return Err(TypeError::TypeMismatch {
                    expected: Type::String,
                    got: ty2,
                });
            }
            Ok(Type::String)
        }

        // (STRLEN)
        // Γ ⊢ t : String
        // ──────────────────
        // Γ ⊢ strlen t : Nat
        Term::StrLen(t) => {
            let ty = type_of(ctx, t)?;
            if ty != Type::String {
                return Err(TypeError::TypeMismatch {
                    expected: Type::String,
                    got: ty,
                });
            }
            Ok(Type::Nat)
        }

        // (STREQ)
        // Γ ⊢ t₁ : String   Γ ⊢ t₂ : String
        // ──────────────────────────────────
        // Γ ⊢ streq t₁ t₂ : Bool
        Term::StrEq(t1, t2) => {
            let ty1 = type_of(ctx, t1)?;
            let ty2 = type_of(ctx, t2)?;
            if ty1 != Type::String {
                return Err(TypeError::TypeMismatch {
                    expected: Type::String,
                    got: ty1,
                });
            }
            if ty2 != Type::String {
                return Err(TypeError::TypeMismatch {
                    expected: Type::String,
                    got: ty2,
                });
            }
            Ok(Type::Bool)
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: General Recursion
        // ═══════════════════════════════════════════════════════════════════

        // (FIX)
        // Γ, f:τ ⊢ t : τ
        // ─────────────────────
        // Γ ⊢ fix f:τ. t : τ
        Term::Fix(f, ty, body) => {
            check_type_wf(ctx, ty)?;
            // Add f to context with type τ
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: Recursive Types (μ-types)
        // ═══════════════════════════════════════════════════════════════════

        // (FOLD)
        // Γ ⊢ t : τ[α := μα.τ]
        // ─────────────────────────
        // Γ ⊢ fold [μα.τ] t : μα.τ
        Term::Fold(mu_ty, t) => {
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

        // (UNFOLD)
        // Γ ⊢ t : μα.τ
        // ─────────────────────────────
        // Γ ⊢ unfold [μα.τ] t : τ[α := μα.τ]
        Term::Unfold(mu_ty, t) => {
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
                    // Result type is body with α replaced by μα.body
                    Ok(body.substitute(alpha, mu_ty))
                }
                _ => Err(TypeError::MalformedType(mu_ty.clone())),
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3C: Nat Arithmetic
        // ═══════════════════════════════════════════════════════════════════

        // (NAT-ADD) / (NAT-SUB) / (NAT-MUL) / (NAT-DIV) / (NAT-MOD)
        // Γ ⊢ t₁ : Nat   Γ ⊢ t₂ : Nat
        // ─────────────────────────────
        // Γ ⊢ t₁ op t₂ : Nat
        Term::NatAdd(t1, t2)
        | Term::NatSub(t1, t2)
        | Term::NatMul(t1, t2)
        | Term::NatDiv(t1, t2)
        | Term::NatMod(t1, t2) => {
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

        // (NAT-EQ)
        // Γ ⊢ t₁ : Nat   Γ ⊢ t₂ : Nat
        // ─────────────────────────────
        // Γ ⊢ t₁ == t₂ : Bool
        Term::NatEq(t1, t2) => {
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: Integer Comparison
        // ═══════════════════════════════════════════════════════════════════

        // (NAT-LT) / (NAT-LE) / (NAT-GT) / (NAT-GE)
        // Γ ⊢ t₁ : Nat   Γ ⊢ t₂ : Nat
        // ─────────────────────────────
        // Γ ⊢ t₁ < t₂ : Bool
        Term::NatLt(t1, t2) | Term::NatLe(t1, t2) | Term::NatGt(t1, t2) | Term::NatGe(t1, t2) => {
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3C: Boolean Operations
        // ═══════════════════════════════════════════════════════════════════

        // (BOOL-AND) / (BOOL-OR)
        // Γ ⊢ t₁ : Bool   Γ ⊢ t₂ : Bool
        // ─────────────────────────────
        // Γ ⊢ t₁ && t₂ : Bool
        Term::BoolAnd(t1, t2) | Term::BoolOr(t1, t2) => {
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

        // (BOOL-NOT)
        // Γ ⊢ t : Bool
        // ────────────────
        // Γ ⊢ !t : Bool
        Term::BoolNot(t) => {
            let ty = type_of(ctx, t)?;
            if !types_equal(&ty, &Type::Bool) {
                return Err(TypeError::TypeMismatch {
                    expected: Type::Bool,
                    got: ty,
                });
            }
            Ok(Type::Bool)
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: String character access
        // ═══════════════════════════════════════════════════════════════════

        // (STR-CHAR-AT)
        // Γ ⊢ s : String   Γ ⊢ n : Nat
        // ─────────────────────────────
        // Γ ⊢ char_at s n : Nat
        Term::StrCharAt(s, n) => {
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
            Ok(Type::Nat) // Returns ASCII code
        }

        // (STR-SUBSTRING)
        // Γ ⊢ s : String   Γ ⊢ start : Nat   Γ ⊢ len : Nat
        // ────────────────────────────────────────────────
        // Γ ⊢ substring s start len : String
        Term::StrSubstring(s, start, len) => {
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: FFI (External Calls)
        // ═══════════════════════════════════════════════════════════════════

        // ExternCall: type checking requires extern function signatures in context
        // For now, we treat it as stuck/sorry - requires elaborator integration
        Term::ExternCall(_, _) => {
            // In the core calculus, ExternCall is opaque
            // The elaborator must ensure types are correct
            // Here we just return Unit as a placeholder
            Ok(Type::Unit)
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: Ref Cells
        // ═══════════════════════════════════════════════════════════════════

        // (REF-NEW)
        // Γ ⊢ t : τ
        // ─────────────────
        // Γ ⊢ ref t : Ref<τ>
        Term::RefNew(t) => {
            let t_ty = type_of(ctx, t)?;
            Ok(Type::ref_ty(t_ty))
        }

        // (REF-GET)
        // Γ ⊢ t : Ref<τ>
        // ───────────────
        // Γ ⊢ get t : τ
        Term::RefGet(t) => {
            let t_ty = type_of(ctx, t)?;
            match t_ty {
                Type::Ref(inner) => Ok(*inner),
                _ => Err(TypeError::TypeMismatch {
                    expected: Type::ref_ty(Type::TyVar("τ".into())),
                    got: t_ty,
                }),
            }
        }

        // (REF-SET)
        // Γ ⊢ r : Ref<τ>   Γ ⊢ v : τ
        // ───────────────────────────
        // Γ ⊢ set r v : Unit
        Term::RefSet(r, v) => {
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2B: Flat ADT (ADR 2.2.26)
        // ═══════════════════════════════════════════════════════════════════

        // (ADT-CONSTRUCT)
        // adt_ty = Adt(name, args, variants)
        // variants[idx] = (ctor_name, payload_ty)
        // Γ ⊢ payload : payload_ty
        // ───────────────────────────────────────────
        // Γ ⊢ adt_construct [adt_ty] idx payload : adt_ty
        Term::AdtConstruct(adt_ty, idx, payload) => {
            check_type_wf(ctx, adt_ty)?;
            match adt_ty {
                Type::Adt(_name, _type_args, variants) => {
                    if *idx >= variants.len() {
                        return Err(TypeError::InvalidVariantIndex {
                            index: *idx,
                            num_variants: variants.len(),
                        });
                    }
                    let (_ctor_name, expected_payload_ty) = &variants[*idx];
                    let payload_ty = type_of(ctx, payload)?;
                    if !types_equal(&payload_ty, expected_payload_ty) {
                        return Err(TypeError::TypeMismatch {
                            expected: expected_payload_ty.clone(),
                            got: payload_ty,
                        });
                    }
                    Ok(adt_ty.clone())
                }
                _ => Err(TypeError::NotAnAdt {
                    got: adt_ty.clone(),
                }),
            }
        }

        // (ADT-MATCH)
        // Γ ⊢ scrut : Adt(name, args, variants)
        // For each arm (idx, var, body):
        //   variants[idx] = (_, payload_ty)
        //   Γ, var:payload_ty ⊢ body : τ
        // All arms have same result type τ
        // ─────────────────────────────────────────────────
        // Γ ⊢ adt_match scrut arms : τ
        Term::AdtMatch(scrut, arms) => {
            let scrut_ty = type_of(ctx, scrut)?;
            match &scrut_ty {
                Type::Adt(_name, _type_args, variants) => {
                    if arms.is_empty() {
                        return Err(TypeError::EmptyMatch);
                    }

                    let mut result_ty: Option<Type> = None;

                    for (idx, var, body) in arms {
                        if *idx >= variants.len() {
                            return Err(TypeError::InvalidVariantIndex {
                                index: *idx,
                                num_variants: variants.len(),
                            });
                        }
                        let (_ctor_name, payload_ty) = &variants[*idx];

                        // Bind the payload variable and typecheck body
                        let arm_ctx = ctx.with_term(var, payload_ty.clone());
                        let body_ty = type_of(&arm_ctx, body)?;

                        match &result_ty {
                            None => result_ty = Some(body_ty),
                            Some(expected) => {
                                if !types_equal(&body_ty, expected) {
                                    return Err(TypeError::BranchTypeMismatch {
                                        then_type: expected.clone(),
                                        else_type: body_ty,
                                    });
                                }
                            }
                        }
                    }

                    Ok(result_ty.unwrap())
                }
                _ => Err(TypeError::NotAnAdt { got: scrut_ty }),
            }
        }
    }
}
