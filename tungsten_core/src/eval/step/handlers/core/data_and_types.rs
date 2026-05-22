//! Step handlers for pairs, projections, case, subst, type application,
//! annotation, and recursive type unfold.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::step::step;
use crate::eval::StepResult;
/// Step Pair: evaluate both components to values.
pub(in crate::eval) fn step_pair(t1: &Term, t2: &Term) -> StepResult {
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::pair(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::pair(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    StepResult::Value
}

/// Step Fst: evaluate argument, then project first component.
pub(in crate::eval) fn step_fst(t: &Term) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::fst(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Pair(v1, _) => StepResult::Stepped(v1.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}

/// Step Snd: evaluate argument, then project second component.
pub(in crate::eval) fn step_snd(t: &Term) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::snd(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Pair(_, v2) => StepResult::Stepped(v2.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}

/// Step a case expression: evaluate scrutinee, then branch on Inl/Inr.
pub(in crate::eval) fn step_case(
    scrut: &Term,
    x: &str,
    left: &Term,
    y: &str,
    right: &Term,
) -> StepResult {
    if !scrut.is_value() {
        match step(scrut) {
            StepResult::Stepped(scrut_new) => {
                return StepResult::Stepped(Term::case(
                    scrut_new,
                    x.to_string(),
                    left.clone(),
                    y.to_string(),
                    right.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match scrut {
        Term::Inl(_, v) => {
            let result = left.substitute(x, v);
            StepResult::Stepped(result)
        }
        Term::Inr(_, v) => {
            let result = right.substitute(y, v);
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}

/// Step a substitution (equality transport): evaluate proof, then eliminate.
pub(in crate::eval) fn step_subst(
    ty: &Type,
    motive: &Type,
    eq_proof: &Term,
    proof: &Term,
) -> StepResult {
    if !eq_proof.is_value() {
        match step(eq_proof) {
            StepResult::Stepped(eq_new) => {
                return StepResult::Stepped(Term::subst(
                    ty.clone(),
                    motive.clone(),
                    eq_new,
                    proof.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !proof.is_value() {
        match step(proof) {
            StepResult::Stepped(proof_new) => {
                return StepResult::Stepped(Term::subst(
                    ty.clone(),
                    motive.clone(),
                    eq_proof.clone(),
                    proof_new,
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match eq_proof {
        Term::Refl(_, _) => StepResult::Stepped(proof.clone()),
        _ => StepResult::Stuck,
    }
}

/// Step TyApp: evaluate, then erase type abstraction.
pub(in crate::eval) fn step_tyapp(t: &Term, ty: &Type) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => {
                return StepResult::Stepped(Term::ty_app(t_new, ty.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::TyAbs(_, body) => StepResult::Stepped(body.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}

/// Step Annot: strip annotation, evaluate inner term.
pub(in crate::eval) fn step_annot(t: &Term) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(t_new),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    StepResult::Stepped(t.clone())
}

/// Step Unfold: evaluate argument, then unwrap Fold.
pub(in crate::eval) fn step_unfold(t: &Term, ty: &Type) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => {
                return StepResult::Stepped(Term::unfold(ty.clone(), t_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Fold(_, inner) => StepResult::Stepped(inner.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}
