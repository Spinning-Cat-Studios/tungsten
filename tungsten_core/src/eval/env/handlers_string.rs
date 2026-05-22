//! String operation handlers for step_with_env().
//!
//! Handles: StrLen, StrConcat, StrEq, StrCharAt, StrSubstring

use crate::terms::Term;

use super::step_with_env;
use super::EvalEnv;
use crate::eval::helpers::{nat_to_term, term_to_nat};
use crate::eval::StepResult;

/// Step StrLen with environment: evaluate argument, then compute length.
pub(super) fn step_str_len_env(t: &Term, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::str_len(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::StringLit(s) => StepResult::Stepped(nat_to_term(s.len())),
        _ => StepResult::Stuck,
    }
}

/// Step string concatenation with environment.
pub(super) fn step_str_concat_env(t1: &Term, t2: &Term, env: &EvalEnv) -> StepResult {
    if !t1.is_value() {
        match step_with_env(t1, env) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::str_concat(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step_with_env(t2, env) {
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

/// Step string equality with environment.
pub(super) fn step_str_eq_env(t1: &Term, t2: &Term, env: &EvalEnv) -> StepResult {
    if !t1.is_value() {
        match step_with_env(t1, env) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::str_eq(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step_with_env(t2, env) {
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

/// Step string character access with environment.
pub(super) fn step_str_char_at_env(s: &Term, n: &Term, env: &EvalEnv) -> StepResult {
    if !s.is_value() {
        match step_with_env(s, env) {
            StepResult::Stepped(s_new) => {
                return StepResult::Stepped(Term::str_char_at(s_new, n.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !n.is_value() {
        match step_with_env(n, env) {
            StepResult::Stepped(n_new) => {
                return StepResult::Stepped(Term::str_char_at(s.clone(), n_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (s, term_to_nat(n)) {
        (Term::StringLit(str_val), Some(idx)) => {
            if let Some(ch) = str_val.chars().nth(idx) {
                StepResult::Stepped(nat_to_term(ch as usize))
            } else {
                StepResult::Stepped(Term::Zero) // Out of bounds → 0
            }
        }
        _ => StepResult::Stuck,
    }
}

/// Step string substring with environment.
pub(super) fn step_str_substring_env(
    s: &Term,
    start: &Term,
    len: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !s.is_value() {
        match step_with_env(s, env) {
            StepResult::Stepped(s_new) => {
                return StepResult::Stepped(Term::str_substring(s_new, start.clone(), len.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !start.is_value() {
        match step_with_env(start, env) {
            StepResult::Stepped(start_new) => {
                return StepResult::Stepped(Term::str_substring(s.clone(), start_new, len.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !len.is_value() {
        match step_with_env(len, env) {
            StepResult::Stepped(len_new) => {
                return StepResult::Stepped(Term::str_substring(s.clone(), start.clone(), len_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match (s, term_to_nat(start), term_to_nat(len)) {
        (Term::StringLit(str_val), Some(start_idx), Some(length)) => {
            let chars: Vec<char> = str_val.chars().collect();
            let result: String = chars.iter().skip(start_idx).take(length).collect();
            StepResult::Stepped(Term::string_lit(result))
        }
        _ => StepResult::Stuck,
    }
}
