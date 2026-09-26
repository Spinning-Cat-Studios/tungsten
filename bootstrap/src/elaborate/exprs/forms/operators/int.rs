//! The `Int` half of operator elaboration (ADR 14.9.26c §2.2).
//!
//! Arithmetic and ordering lower to one `IntBin` node with the operator on
//! it; equality is `IntBin(Eq)`; negation is `IntNeg`, or a folded literal
//! when the operand is one (which is how `MIN` is spelled). Literals are
//! range-checked here — `E0090` — because the parser carries a `u64`.

use crate::ast::{BinOp, Expr};
use crate::span::{Span, Spanned};
use tungsten_core::terms::IntBinOp;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// The `IntBinOp` an arithmetic or ordering `BinOp` lowers to.
fn int_bin_op(op: BinOp) -> IntBinOp {
    match op {
        BinOp::Add => IntBinOp::Add,
        BinOp::Sub => IntBinOp::Sub,
        BinOp::Mul => IntBinOp::Mul,
        BinOp::Div => IntBinOp::Div,
        BinOp::Mod => IntBinOp::Mod,
        BinOp::Lt => IntBinOp::Lt,
        BinOp::Le => IntBinOp::Le,
        BinOp::Gt => IntBinOp::Gt,
        BinOp::Ge => IntBinOp::Ge,
        BinOp::Eq => IntBinOp::Eq,
        other => unreachable!("int_bin_op called with {other:?}"),
    }
}

/// The signed value of a literal, or `None` when it does not fit `Int`.
///
/// `negated` admits one more magnitude — `9223372036854775808` is out of
/// range, `-9223372036854775808` is `MIN`.
pub(in crate::elaborate::exprs) fn int_literal_value(magnitude: u64, negated: bool) -> Option<i64> {
    let signed = i128::from(magnitude);
    let value = if negated { -signed } else { signed };
    i64::try_from(value).ok()
}

/// E0090 for a literal outside `Int`'s range — shared with integer `match`
/// patterns (ADR 18.9.26e), which range-check the same `u64` magnitude.
pub(in crate::elaborate::exprs) fn int_literal_out_of_range(
    magnitude: u64,
    negated: bool,
    span: Span,
) -> ElabError {
    let sign = if negated { "-" } else { "" };
    ElabError::new(
        span,
        ElabErrorKind::IntLiteralOutOfRange(format!("{sign}{magnitude}")),
    )
}

impl<'a> Elaborator<'a> {
    /// An integer literal checked against `Int`, folded with its sign.
    pub(super) fn int_literal(
        &self,
        magnitude: u64,
        negated: bool,
        span: Span,
    ) -> ElabResult<Term> {
        match int_literal_value(magnitude, negated) {
            Some(value) => Ok(Term::int_lit(value)),
            None => Err(int_literal_out_of_range(magnitude, negated, span)),
        }
    }

    /// Build an `Int` arithmetic or ordering node from two checked operands.
    pub(super) fn build_int_binary(&self, op: BinOp, left: Term, right: Term) -> (Term, Type) {
        let op = int_bin_op(op);
        let result_ty = if op.is_comparison() {
            Type::Bool
        } else {
            Type::Int
        };
        (Term::int_bin(op, left, right), result_ty)
    }

    /// `Int` equality: the `IntBin(Eq)` primitive.
    pub(super) fn build_int_equality(left: Term, right: Term) -> Term {
        Term::int_bin(IntBinOp::Eq, left, right)
    }

    /// Elaborate unary minus: a bare literal folds to an `Int` literal, an
    /// `Int` operand negates, and a `Nat` operand keeps its refusal — reworded
    /// to point at `Int`.
    pub(super) fn elab_negation(&mut self, operand: &Expr, span: Span) -> ElabResult<(Term, Type)> {
        if let Some(magnitude) = super::bare_literal(operand) {
            return Ok((self.int_literal(magnitude, true, span)?, Type::Int));
        }
        let (term, ty) = self.infer(operand)?;
        match ty {
            Type::Int => Ok((Term::int_neg(term), Type::Int)),
            Type::Nat => Err(ElabError::unsupported(span, "numeric negation of a Nat")
                .with_help("Nat has no negative numbers; negate an Int — `-to_int(n)`")),
            // Poison passes through: the operand's own error was reported.
            Type::Error => Ok((Term::int_neg(term), Type::Error)),
            other => Err(self.type_mismatch_error(operand.span(), Type::Int, other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 14.9.26c AC 1: the signed range, at both edges and one past each.
    #[test]
    fn literal_range_admits_min_and_max_and_refuses_one_past_each() {
        assert_eq!(int_literal_value(0, false), Some(0));
        assert_eq!(int_literal_value(0, true), Some(0));
        assert_eq!(int_literal_value(i64::MAX as u64, false), Some(i64::MAX));
        assert_eq!(int_literal_value(i64::MAX as u64 + 1, false), None);
        assert_eq!(int_literal_value(i64::MAX as u64 + 1, true), Some(i64::MIN));
        assert_eq!(int_literal_value(i64::MAX as u64 + 2, true), None);
        assert_eq!(int_literal_value(u64::MAX, false), None);
        assert_eq!(int_literal_value(u64::MAX, true), None);
    }

    /// Every arithmetic and ordering operator maps to its own `IntBinOp`.
    #[test]
    fn each_bin_op_maps_to_its_own_int_op() {
        let table = [
            (BinOp::Add, IntBinOp::Add),
            (BinOp::Sub, IntBinOp::Sub),
            (BinOp::Mul, IntBinOp::Mul),
            (BinOp::Div, IntBinOp::Div),
            (BinOp::Mod, IntBinOp::Mod),
            (BinOp::Lt, IntBinOp::Lt),
            (BinOp::Le, IntBinOp::Le),
            (BinOp::Gt, IntBinOp::Gt),
            (BinOp::Ge, IntBinOp::Ge),
            (BinOp::Eq, IntBinOp::Eq),
        ];
        for (bin, int) in table {
            assert_eq!(int_bin_op(bin), int, "{bin:?}");
        }
    }
}
