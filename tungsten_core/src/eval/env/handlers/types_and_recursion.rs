//! Step handlers for type application, annotation, and recursive type unfold.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::env::step_with_env;
use crate::eval::env::EvalEnv;
use crate::eval::StepResult;
/// Step TyApp with environment: evaluate, then erase type abstraction.
pub(in crate::eval::env) fn step_tyapp_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
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

/// Step Annot with environment: strip annotation, evaluate inner term.
pub(in crate::eval::env) fn step_annot_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if t.is_value() {
        StepResult::Stepped(t.clone())
    } else {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => StepResult::Stepped(Term::annot(t_new, ty.clone())),
            other => other,
        }
    }
}

/// Step Unfold with environment: evaluate argument, then unwrap Fold.
pub(in crate::eval::env) fn step_unfold_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
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
