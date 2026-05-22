//! Extended step handlers: strings, booleans, and ADT operations.

use crate::terms::Term;

use crate::eval::helpers::{nat_to_term, term_to_nat};
use crate::eval::step::step;
use crate::eval::StepResult;
// ============================================================================
// String operations
// ============================================================================

/// Step string concatenation.
pub(in crate::eval) fn step_str_concat(t1: &Term, t2: &Term) -> StepResult {
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::str_concat(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::str_concat(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match (t1, t2) {
        (Term::StringLit(s1), Term::StringLit(s2)) => {
            StepResult::Stepped(Term::string_lit(format!("{s1}{s2}")))
        }
        _ => StepResult::Stuck,
    }
}

/// Step string equality comparison.
pub(in crate::eval) fn step_str_eq(t1: &Term, t2: &Term) -> StepResult {
    if !t1.is_value() {
        match step(t1) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::str_eq(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step(t2) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::str_eq(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match (t1, t2) {
        (Term::StringLit(s1), Term::StringLit(s2)) => {
            if s1 == s2 {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

/// Step StrLen: evaluate argument, then compute length.
pub(in crate::eval) fn step_str_len(t: &Term) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::str_len(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::StringLit(s) => StepResult::Stepped(Term::nat(s.len() as u64)),
        _ => StepResult::Stuck,
    }
}

/// Step string character access.
pub(in crate::eval) fn step_str_char_at(s: &Term, idx: &Term) -> StepResult {
    if !s.is_value() {
        match step(s) {
            StepResult::Stepped(s_new) => {
                return StepResult::Stepped(Term::str_char_at(s_new, idx.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !idx.is_value() {
        match step(idx) {
            StepResult::Stepped(idx_new) => {
                return StepResult::Stepped(Term::str_char_at(s.clone(), idx_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match (s, term_to_nat(idx)) {
        (Term::StringLit(str), Some(n)) => {
            if n < str.len() {
                let ch = str.as_bytes()[n];
                StepResult::Stepped(nat_to_term(ch as usize))
            } else {
                StepResult::Stuck
            }
        }
        _ => StepResult::Stuck,
    }
}

/// Step string substring extraction.
pub(in crate::eval) fn step_str_substring(s: &Term, start: &Term, len: &Term) -> StepResult {
    if !s.is_value() {
        match step(s) {
            StepResult::Stepped(s_new) => {
                return StepResult::Stepped(Term::str_substring(s_new, start.clone(), len.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !start.is_value() {
        match step(start) {
            StepResult::Stepped(start_new) => {
                return StepResult::Stepped(Term::str_substring(s.clone(), start_new, len.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !len.is_value() {
        match step(len) {
            StepResult::Stepped(len_new) => {
                return StepResult::Stepped(Term::str_substring(s.clone(), start.clone(), len_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match (s, term_to_nat(start), term_to_nat(len)) {
        (Term::StringLit(str), Some(start_idx), Some(length)) => {
            let end_idx = (start_idx + length).min(str.len());
            let start_idx = start_idx.min(str.len());
            let result = &str[start_idx..end_idx];
            StepResult::Stepped(Term::string_lit(result))
        }
        _ => StepResult::Stuck,
    }
}

// ============================================================================
// Boolean / ADT operations
// ============================================================================

/// Step BoolNot: evaluate argument, then negate.
pub(in crate::eval) fn step_bool_not(t: &Term) -> StepResult {
    if !t.is_value() {
        match step(t) {
            StepResult::Stepped(t_new) => {
                return StepResult::Stepped(Term::BoolNot(Box::new(t_new)));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::True => StepResult::Stepped(Term::False),
        Term::False => StepResult::Stepped(Term::True),
        _ => StepResult::Stuck,
    }
}

/// Step an ADT match expression.
pub(in crate::eval) fn step_adt_match(
    scrut: &Term,
    arms: &[(usize, String, Box<Term>)],
) -> StepResult {
    if !scrut.is_value() {
        match step(scrut) {
            StepResult::Stepped(scrut_new) => {
                return StepResult::Stepped(Term::adt_match(scrut_new, arms.to_vec()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match scrut {
        Term::AdtConstruct(_, tag, payload) => {
            for (arm_idx, var, body) in arms {
                if arm_idx == tag {
                    let result = body.substitute(var, payload);
                    return StepResult::Stepped(result);
                }
            }
            StepResult::Stuck
        }
        _ => StepResult::Stuck,
    }
}
