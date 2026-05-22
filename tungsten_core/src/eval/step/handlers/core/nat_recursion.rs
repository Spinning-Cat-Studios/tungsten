//! Step handlers for NatRec and NatInd.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::step::step;
use crate::eval::StepResult;
/// Step NatRec: primitive recursion on natural numbers.
pub(in crate::eval) fn step_natrec(
    ty: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
) -> StepResult {
    if !n.is_value() {
        match step(n) {
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

/// Step NatInd: same semantics as NatRec (proofs compute the same way).
pub(in crate::eval) fn step_natind(
    motive: &Type,
    zero_case: &Term,
    succ_case: &Term,
    n: &Term,
) -> StepResult {
    if !n.is_value() {
        match step(n) {
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
