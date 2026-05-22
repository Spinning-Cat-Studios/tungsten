use super::*;
use crate::eval::eval;
use crate::types::Type;

// ==========================================================================
// Phase 3-Prep Tests: Integer Comparison
// ==========================================================================

#[test]
fn test_nat_lt_true() {
    let term = Term::nat_lt(Term::nat(2), Term::nat(5));
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_nat_lt_false() {
    let term = Term::nat_lt(Term::nat(5), Term::nat(2));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

#[test]
fn test_nat_lt_equal() {
    let term = Term::nat_lt(Term::nat(3), Term::nat(3));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

#[test]
fn test_nat_le_true() {
    let term = Term::nat_le(Term::nat(3), Term::nat(3));
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_nat_le_false() {
    let term = Term::nat_le(Term::nat(5), Term::nat(3));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

#[test]
fn test_nat_gt_true() {
    let term = Term::nat_gt(Term::nat(5), Term::nat(3));
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_nat_gt_false() {
    let term = Term::nat_gt(Term::nat(3), Term::nat(5));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

#[test]
fn test_nat_ge_true() {
    let term = Term::nat_ge(Term::nat(5), Term::nat(5));
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_nat_ge_false() {
    let term = Term::nat_ge(Term::nat(3), Term::nat(5));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

// ==========================================================================
// Phase 3-Prep Tests: String char_at
// ==========================================================================

#[test]
fn test_str_char_at() {
    let term = Term::str_char_at(Term::string_lit("hello"), Term::Zero);
    let result = eval(&term);
    assert_eq!(result, Term::nat(104));
}

#[test]
fn test_str_char_at_middle() {
    let term = Term::str_char_at(Term::string_lit("hello"), Term::nat(2));
    let result = eval(&term);
    assert_eq!(result, Term::nat(108));
}

#[test]
fn test_str_char_at_out_of_bounds() {
    let term = Term::str_char_at(Term::string_lit("hi"), Term::nat(5));
    let result = step(&term);
    assert_eq!(result, StepResult::Stuck);
}

// ==========================================================================
// Phase 3-Prep Tests: FFI and Ref Cells (stuck in pure evaluation)
// ==========================================================================

#[test]
fn test_extern_call_stuck() {
    let term = Term::extern_call("puts", vec![Term::string_lit("hello")]);
    let result = step(&term);
    assert_eq!(result, StepResult::Stuck);
}

#[test]
fn test_ref_new_stuck() {
    let term = Term::ref_new(Term::nat(42));
    let result = step(&term);
    assert_eq!(result, StepResult::Stuck);
}

#[test]
fn test_ref_get_stuck() {
    let term = Term::ref_get(Term::var("r"));
    let result = step(&term);
    assert_eq!(result, StepResult::Stuck);
}

#[test]
fn test_ref_set_stuck() {
    let term = Term::ref_set(Term::var("r"), Term::nat(42));
    let result = step(&term);
    assert_eq!(result, StepResult::Stuck);
}
