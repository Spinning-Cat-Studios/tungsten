//! Signed-integer stepping: checked, trapping (ADR 14.9.26c §2.3).
//!
//! Every arithmetic arm uses `checked_*`; a `None` records
//! [`EvalStopped::IntTrap`] on the env and returns `Stuck`, exactly the way a
//! comparison that never ran does — interior stepping terminates, and the
//! entry point surfaces the trap rather than a value. Native code aborts
//! through `tg_int_trap(kind)` with the same line, which is the parity
//! contract `diff exec` checks.

use crate::eval::env::{step_with_env, EvalEnv, EvalStopped, IntTrapKind};
use crate::eval::StepResult;
use crate::terms::{IntBinOp, Term};

/// The `IntBin` / `IntNeg` / `NatToInt` / `IntToNat` arm of `step_with_env`.
pub(in crate::eval::env) fn step_int_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::IntBin(op, a, b) => step_binary_int_env(*op, a, b, env),
        Term::IntNeg(a) => step_unary_int_env(a, Term::int_neg, negate, env),
        Term::NatToInt(a) => step_unary_int_env(a, Term::nat_to_int, nat_to_int, env),
        Term::IntToNat(a) => step_unary_int_env(a, Term::int_to_nat, int_to_nat, env),
        _ => unreachable!("step_int_env called with a non-Int term"),
    }
}

/// Apply one signed binary operator to two values.
///
/// Pure — the table `step_binary_int_env` and the AC 4 tests share — so a
/// trap is a *returned* fact, never a recorded one, at this layer.
pub fn apply_int_bin(op: IntBinOp, x: i64, y: i64) -> Result<Term, IntTrapKind> {
    let overflow = IntTrapKind::Overflow(op);
    let value = match op {
        IntBinOp::Add => x.checked_add(y).ok_or(overflow)?,
        IntBinOp::Sub => x.checked_sub(y).ok_or(overflow)?,
        IntBinOp::Mul => x.checked_mul(y).ok_or(overflow)?,
        IntBinOp::Div => checked_div_or_mod(x, y, i64::checked_div, overflow)?,
        IntBinOp::Mod => checked_div_or_mod(x, y, i64::checked_rem, overflow)?,
        IntBinOp::Eq => return Ok(bool_term(x == y)),
        IntBinOp::Lt => return Ok(bool_term(x < y)),
        IntBinOp::Le => return Ok(bool_term(x <= y)),
        IntBinOp::Gt => return Ok(bool_term(x > y)),
        IntBinOp::Ge => return Ok(bool_term(x >= y)),
    };
    Ok(Term::IntLit(value))
}

/// `/` and `%` share two traps: a zero divisor, and `MIN / -1`, which
/// `checked_div`/`checked_rem` report as `None` too — so the divisor is
/// tested first to keep the two messages distinct.
fn checked_div_or_mod(
    x: i64,
    y: i64,
    op: fn(i64, i64) -> Option<i64>,
    overflow: IntTrapKind,
) -> Result<i64, IntTrapKind> {
    if y == 0 {
        return Err(IntTrapKind::DivisionByZero);
    }
    op(x, y).ok_or(overflow)
}

/// Signed negation; traps on `MIN`.
pub fn negate(x: &Term) -> Result<Term, IntTrapKind> {
    match x {
        Term::IntLit(v) => v
            .checked_neg()
            .map(Term::IntLit)
            .ok_or(IntTrapKind::NegationOverflow),
        _ => Err(IntTrapKind::NatAboveSignedMax),
    }
}

/// `to_int(n)`: the identity on the value; traps above `i64::MAX`.
pub fn nat_to_int(n: &Term) -> Result<Term, IntTrapKind> {
    let value = crate::eval::term_to_nat(n).ok_or(IntTrapKind::NatAboveSignedMax)?;
    i64::try_from(value)
        .map(Term::IntLit)
        .map_err(|_| IntTrapKind::NatAboveSignedMax)
}

/// `from_int(i)`: clamps negatives to 0 (Lean's `Int.toNat`). Total.
pub fn int_to_nat(i: &Term) -> Result<Term, IntTrapKind> {
    match i {
        Term::IntLit(v) => Ok(crate::eval::nat_to_term(usize::try_from(*v).unwrap_or(0))),
        _ => Err(IntTrapKind::NatAboveSignedMax),
    }
}

fn bool_term(b: bool) -> Term {
    if b {
        Term::True
    } else {
        Term::False
    }
}

/// Step one operand toward a value, rebuilding the node around it.
fn step_operand(
    operand: &Term,
    rebuild: impl FnOnce(Term) -> Term,
    env: &EvalEnv,
) -> Option<StepResult> {
    if operand.is_value() {
        return None;
    }
    Some(match step_with_env(operand, env) {
        StepResult::Stepped(next) => StepResult::Stepped(rebuild(next)),
        StepResult::Stuck => StepResult::Stuck,
        StepResult::Value => return None,
    })
}

fn step_binary_int_env(op: IntBinOp, a: &Term, b: &Term, env: &EvalEnv) -> StepResult {
    if let Some(result) = step_operand(a, |a_new| Term::int_bin(op, a_new, b.clone()), env) {
        return result;
    }
    if let Some(result) = step_operand(b, |b_new| Term::int_bin(op, a.clone(), b_new), env) {
        return result;
    }
    match (a, b) {
        (Term::IntLit(x), Term::IntLit(y)) => trap_or_step(apply_int_bin(op, *x, *y), env),
        _ => StepResult::Stuck,
    }
}

fn step_unary_int_env(
    a: &Term,
    rebuild: fn(Term) -> Term,
    apply: fn(&Term) -> Result<Term, IntTrapKind>,
    env: &EvalEnv,
) -> StepResult {
    if let Some(result) = step_operand(a, rebuild, env) {
        return result;
    }
    trap_or_step(apply(a), env)
}

/// A trap is recorded on the env and the term goes `Stuck`, so the stepping
/// loop terminates and the entry point reports the trap, not a value.
fn trap_or_step(outcome: Result<Term, IntTrapKind>, env: &EvalEnv) -> StepResult {
    match outcome {
        Ok(value) => StepResult::Stepped(value),
        Err(kind) => {
            env.record_stop(EvalStopped::IntTrap { kind });
            StepResult::Stuck
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::env::eval_with_env;

    fn int(v: i64) -> Term {
        Term::IntLit(v)
    }

    fn eval(term: Term) -> Result<Term, EvalStopped> {
        eval_with_env(&term, &EvalEnv::empty())
    }

    fn eval_int(term: Term) -> i64 {
        match eval(term) {
            Ok(Term::IntLit(v)) => v,
            other => panic!("expected an IntLit, got {other:?}"),
        }
    }

    fn trap_of(term: Term) -> IntTrapKind {
        match eval(term) {
            Err(EvalStopped::IntTrap { kind }) => kind,
            other => panic!("expected a trap, got {other:?}"),
        }
    }

    /// 14.9.26c AC 2: the signed results the ADR pins, on the evaluator side.
    #[test]
    fn signed_arithmetic_matches_the_adr_table() {
        assert_eq!(eval_int(Term::int_bin(IntBinOp::Add, int(-3), int(5))), 2);
        assert_eq!(eval_int(Term::int_bin(IntBinOp::Sub, int(3), int(5))), -2);
        assert_eq!(eval_int(Term::int_bin(IntBinOp::Div, int(-7), int(2))), -3);
        assert_eq!(eval_int(Term::int_bin(IntBinOp::Mod, int(-7), int(2))), -1);
        assert_eq!(eval_int(Term::int_bin(IntBinOp::Mul, int(-7), int(2))), -14);
        assert_eq!(eval_int(Term::int_neg(int(4))), -4);
    }

    /// Each comparison at all three orderings, as the `Nat` tests do — a
    /// swapped relation agrees with the original on most operand pairs.
    #[test]
    fn signed_comparisons_distinguish_less_equal_and_greater() {
        for (op, lt, eq, gt) in [
            (IntBinOp::Lt, true, false, false),
            (IntBinOp::Le, true, true, false),
            (IntBinOp::Gt, false, false, true),
            (IntBinOp::Ge, false, true, true),
            (IntBinOp::Eq, false, true, false),
        ] {
            let is_true = |a, b| matches!(eval(Term::int_bin(op, int(a), int(b))), Ok(Term::True));
            assert_eq!(is_true(-1, 0), lt, "-1 {op:?} 0");
            assert_eq!(is_true(4, 4), eq, "4 {op:?} 4");
            assert_eq!(is_true(0, -1), gt, "0 {op:?} -1");
        }
    }

    /// 14.9.26c AC 4: the checked arms record the stop on every boundary.
    #[test]
    fn overflow_and_division_record_the_named_trap() {
        let min = int(i64::MIN);
        let max = int(i64::MAX);
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Add, max.clone(), int(1))),
            IntTrapKind::Overflow(IntBinOp::Add)
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Sub, min.clone(), int(1))),
            IntTrapKind::Overflow(IntBinOp::Sub)
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Mul, min.clone(), int(-1))),
            IntTrapKind::Overflow(IntBinOp::Mul)
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Div, int(7), int(0))),
            IntTrapKind::DivisionByZero
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Mod, int(7), int(0))),
            IntTrapKind::DivisionByZero
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Div, min.clone(), int(-1))),
            IntTrapKind::Overflow(IntBinOp::Div)
        );
        assert_eq!(
            trap_of(Term::int_bin(IntBinOp::Mod, min.clone(), int(-1))),
            IntTrapKind::Overflow(IntBinOp::Mod)
        );
        assert_eq!(trap_of(Term::int_neg(min)), IntTrapKind::NegationOverflow);
    }

    /// The trap surfaces from a NESTED position too: the operand steps to a
    /// value, then the enclosing node traps.
    #[test]
    fn a_nested_trap_is_reported_not_swallowed() {
        let nested = Term::int_bin(
            IntBinOp::Add,
            int(0),
            Term::int_bin(IntBinOp::Add, int(i64::MAX), int(1)),
        );
        assert_eq!(trap_of(nested), IntTrapKind::Overflow(IntBinOp::Add));
    }

    #[test]
    fn bridges_clamp_and_trap_at_the_documented_edges() {
        assert_eq!(
            eval(Term::int_to_nat(int(-1))).map(|t| crate::eval::term_to_nat(&t)),
            Ok(Some(0))
        );
        assert_eq!(
            eval(Term::int_to_nat(int(42))).map(|t| crate::eval::term_to_nat(&t)),
            Ok(Some(42))
        );
        assert_eq!(eval_int(Term::nat_to_int(Term::NatLit(42))), 42);
        assert_eq!(eval_int(Term::nat_to_int(Term::nat_smart(3))), 3);
        // The signed maximum round-trips through `Nat` and back.
        let max_round_trip = Term::nat_to_int(Term::int_to_nat(int(i64::MAX)));
        assert_eq!(eval_int(max_round_trip), i64::MAX);
        assert_eq!(
            trap_of(Term::nat_to_int(Term::NatLit(i64::MAX as u64 + 1))),
            IntTrapKind::NatAboveSignedMax
        );
    }

    /// The code table round-trips, and the trap line names the operator.
    #[test]
    fn trap_kind_codes_round_trip_and_messages_name_the_operation() {
        for code in 0..8 {
            let kind = IntTrapKind::from_code(code).expect("dense table");
            assert_eq!(kind.code(), code);
        }
        assert_eq!(IntTrapKind::from_code(8), None);
        assert_eq!(
            IntTrapKind::Overflow(IntBinOp::Add).message(),
            "integer overflow in +"
        );
        assert_eq!(
            IntTrapKind::DivisionByZero.message(),
            "integer division by zero"
        );
        assert_eq!(
            EvalStopped::IntTrap {
                kind: IntTrapKind::NegationOverflow
            }
            .to_string(),
            "integer overflow in negation"
        );
    }
}
