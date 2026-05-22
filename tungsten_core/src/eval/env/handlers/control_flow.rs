//! Step handlers for application, let, if, natrec, natind, and case.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::env::step_with_env;
use crate::eval::env::EvalEnv;
use crate::eval::StepResult;
/// A case branch: binding variable name and body term.
pub(in crate::eval) struct CaseArm<'a> {
    pub var: &'a str,
    pub body: &'a Term,
}

/// Step an application with environment.
pub(in crate::eval::env) fn step_app_env(t1: &Term, t2: &Term, env: &EvalEnv) -> StepResult {
    if !t1.is_value() {
        match step_with_env(t1, env) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::app(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !t2.is_value() {
        match step_with_env(t2, env) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::app(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match t1 {
        Term::Lambda(x, _ty, body) => {
            let result = body.substitute(x, t2);
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}

/// Step a let binding with environment.
pub(in crate::eval::env) fn step_let_env(
    x: &str,
    ty: &Type,
    def: &Term,
    body: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !def.is_value() {
        match step_with_env(def, env) {
            StepResult::Stepped(def_new) => {
                return StepResult::Stepped(Term::let_in(
                    x.to_string(),
                    ty.clone(),
                    def_new,
                    body.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    let result = body.substitute(x, def);
    StepResult::Stepped(result)
}

/// Step if-then-else with environment.
pub(in crate::eval::env) fn step_if_env(
    cond: &Term,
    then_: &Term,
    else_: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !cond.is_value() {
        match step_with_env(cond, env) {
            StepResult::Stepped(cond_new) => {
                return StepResult::Stepped(Term::if_then_else(
                    cond_new,
                    then_.clone(),
                    else_.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match cond {
        Term::True => StepResult::Stepped(then_.clone()),
        Term::False => StepResult::Stepped(else_.clone()),
        _ => StepResult::Stuck,
    }
}

/// Step NatRec with environment.
pub(in crate::eval::env) fn step_natrec_env(
    ty: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !n.is_value() {
        match step_with_env(n, env) {
            StepResult::Stepped(n_new) => {
                return StepResult::Stepped(Term::natrec(
                    ty.clone(),
                    zero_case.clone(),
                    succ_case.clone(),
                    n_new,
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match n {
        Term::Zero | Term::NatLit(0) => StepResult::Stepped(zero_case.clone()),
        Term::NatLit(k) => {
            let pred = Term::NatLit(k - 1);
            let rec_call = Term::natrec(
                ty.clone(),
                zero_case.clone(),
                succ_case.clone(),
                pred.clone(),
            );
            let result = Term::app(Term::app(succ_case.clone(), pred), rec_call);
            StepResult::Stepped(result)
        }
        Term::Succ(pred) => {
            let rec_call = Term::natrec(
                ty.clone(),
                zero_case.clone(),
                succ_case.clone(),
                pred.as_ref().clone(),
            );
            let result = Term::app(
                Term::app(succ_case.clone(), pred.as_ref().clone()),
                rec_call,
            );
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}

/// Step NatInd with environment.
pub(in crate::eval::env) fn step_natind_env(
    motive: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !n.is_value() {
        match step_with_env(n, env) {
            StepResult::Stepped(n_new) => {
                return StepResult::Stepped(Term::natind(
                    motive.clone(),
                    zero_case.clone(),
                    succ_case.clone(),
                    n_new,
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match n {
        Term::Zero | Term::NatLit(0) => StepResult::Stepped(zero_case.clone()),
        Term::NatLit(k) => {
            let pred = Term::NatLit(k - 1);
            let rec_call = Term::natind(
                motive.clone(),
                zero_case.clone(),
                succ_case.clone(),
                pred.clone(),
            );
            let result = Term::app(Term::app(succ_case.clone(), pred), rec_call);
            StepResult::Stepped(result)
        }
        Term::Succ(pred) => {
            let rec_call = Term::natind(
                motive.clone(),
                zero_case.clone(),
                succ_case.clone(),
                pred.as_ref().clone(),
            );
            let result = Term::app(
                Term::app(succ_case.clone(), pred.as_ref().clone()),
                rec_call,
            );
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}

/// Step case expression with environment.
pub(in crate::eval::env) fn step_case_env(
    scrut: &Term,
    left: &CaseArm<'_>,
    right: &CaseArm<'_>,
    env: &EvalEnv,
) -> StepResult {
    if !scrut.is_value() {
        match step_with_env(scrut, env) {
            StepResult::Stepped(scrut_new) => {
                return StepResult::Stepped(Term::case(
                    scrut_new,
                    left.var.to_string(),
                    left.body.clone(),
                    right.var.to_string(),
                    right.body.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match scrut {
        Term::Inl(_, v) => {
            let result = left.body.substitute(left.var, v);
            StepResult::Stepped(result)
        }
        Term::Inr(_, v) => {
            let result = right.body.substitute(right.var, v);
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}
