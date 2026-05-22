//! Step handlers for application, let-binding, and if-then-else.

use crate::terms::Term;

use crate::eval::step::step;
use crate::eval::StepResult;
/// Step an application: evaluate function, then argument, then β-reduce.
pub(in crate::eval) fn step_app(t1: &Term, t2: &Term) -> StepResult {
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::app(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::app(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match t1 {
        Term::Lambda(x, _, body) => {
            let result = body.substitute(x, t2);
            StepResult::Stepped(result)
        }
        _ => StepResult::Stuck,
    }
}

/// Step a let binding: evaluate definition, then substitute.
pub(in crate::eval) fn step_let(term: &Term, x: &str, def: &Term, body: &Term) -> StepResult {
    if !def.is_value() {
        match step(def) {
            StepResult::Stepped(def_new) => {
                return StepResult::Stepped(Term::Let(
                    x.to_string(),
                    term.let_type().unwrap().clone(),
                    Box::new(def_new),
                    Box::new(body.clone()),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    StepResult::Stepped(body.substitute(x, def))
}

/// Step an if expression: evaluate condition, then branch.
pub(in crate::eval) fn step_if(cond: &Term, then_: &Term, else_: &Term) -> StepResult {
    if !cond.is_value() {
        match step(cond) {
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
