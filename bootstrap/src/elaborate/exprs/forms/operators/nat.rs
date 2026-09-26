//! The `Nat` half of operator elaboration — today's path, byte-for-byte.
//!
//! `Nat` stays bit-identical under ADR 14.9.26c: saturating subtraction,
//! unsigned comparison, `a == b` as `(a <= b) && (b <= a)`. Only the operand
//! dispatch moved (to `operators/mod.rs`); the terms built here did not.

use crate::ast::BinOp;
use tungsten_core::{Term, Type};

use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Build a `Nat` arithmetic or comparison node from two checked operands.
    pub(super) fn build_nat_binary(
        &mut self,
        op: BinOp,
        left_term: Term,
        right_term: Term,
    ) -> ElabResult<(Term, Type)> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let term = self.build_nat_binop(op, left_term, right_term)?;
                Ok((term, Type::Nat))
            }
            _ => {
                let term = self.build_nat_comparison(op, left_term, right_term)?;
                Ok((term, Type::Bool))
            }
        }
    }

    /// Build arithmetic operation using native primitives.
    ///
    /// These use O(1) machine instructions instead of natrec loops.
    fn build_nat_binop(&mut self, op: BinOp, left: Term, right: Term) -> ElabResult<Term> {
        Ok(match op {
            BinOp::Add => Term::nat_add(left, right),
            BinOp::Sub => Term::nat_sub(left, right),
            BinOp::Mul => Term::nat_mul(left, right),
            BinOp::Div => Term::nat_div(left, right),
            BinOp::Mod => Term::nat_mod(left, right),
            _ => unreachable!("build_nat_binop called with non-arithmetic op"),
        })
    }

    /// Build comparison operation using Phase 3-Prep primitives.
    fn build_nat_comparison(&mut self, op: BinOp, left: Term, right: Term) -> ElabResult<Term> {
        let term = match op {
            BinOp::Lt => Term::nat_lt(left, right),
            BinOp::Le => Term::nat_le(left, right),
            BinOp::Gt => Term::nat_gt(left, right),
            BinOp::Ge => Term::nat_ge(left, right),
            _ => unreachable!("build_nat_comparison called with non-comparison op"),
        };
        Ok(term)
    }

    /// Nat equality: a == b iff (a <= b) && (b <= a)
    /// Implemented as: if (a <= b) then (b <= a) else false
    pub(super) fn build_nat_equality(left: Term, right: Term) -> Term {
        Term::if_then_else(
            Term::nat_le(left.clone(), right.clone()),
            Term::nat_le(right, left),
            Term::False,
        )
    }
}
