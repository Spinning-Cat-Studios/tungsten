//! Operator elaboration.
//!
//! Handles:
//! - Binary operators (arithmetic, comparison, logical, pipe)
//! - Unary operators (not, neg)
//!
//! `operators.rs` became `operators/` when ADR 14.9.26c added the signed
//! `Int` half: `nat.rs` is today's `Nat` path byte-for-byte, `int.rs` the
//! `Int` arithmetic, comparison, equality and negation, and this file the
//! dispatch by operand type.

mod int;
mod nat;

pub(in crate::elaborate::exprs) use int::{int_literal_out_of_range, int_literal_value};

use crate::ast::{BinOp, Expr, UnaryOp};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

/// The five operators that yield a number and the four that yield `Bool`
/// share one operand-type rule; `==`/`!=` have their own (`elab_equality`).
fn is_arith_or_ordering(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Mod
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
    )
}

/// `true` for `+ - * / %`, whose result has the operand type.
fn is_arithmetic(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
    )
}

/// The literal under any number of parentheses, if the expression is one.
fn bare_literal(expr: &Expr) -> Option<u64> {
    match expr {
        Expr::IntLiteral(n, _) => Some(*n),
        Expr::Paren(inner, _) => bare_literal(inner),
        _ => None,
    }
}

impl<'a> Elaborator<'a> {
    /// Elaborate binary operation.
    pub(in crate::elaborate::exprs) fn elab_binary(
        &mut self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        match op {
            // Arithmetic + ordering: the operand type decides Nat or Int
            _ if is_arith_or_ordering(op) => self.elab_arith_binary(left, op, right),

            BinOp::Concat => {
                let left_term = self.check(left, &Type::String)?;
                let right_term = self.check(right, &Type::String)?;
                Ok((Term::str_concat(left_term, right_term), Type::String))
            }

            BinOp::Eq | BinOp::Ne => self.elab_equality(left, op, right),

            BinOp::And | BinOp::Or => {
                let left_term = self.check(left, &Type::Bool)?;
                let right_term = self.check(right, &Type::Bool)?;
                let term = self.build_bool_binop(op, left_term, right_term)?;
                Ok((term, Type::Bool))
            }

            BinOp::Pipe => self.elab_pipe(left, right, span),

            _ => unreachable!("elab_binary: every BinOp is dispatched above"),
        }
    }

    /// Elaborate an arithmetic or ordering operation by operand type (ADR
    /// 14.9.26c): infer the left; if `Int`, check the right against `Int`;
    /// if the left is a bare literal and the right is not, infer the right
    /// first (so `1 + x` works for `x : Int`); otherwise today's `Nat` path.
    fn elab_arith_binary(
        &mut self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
    ) -> ElabResult<(Term, Type)> {
        let left_is_literal = bare_literal(left).is_some();
        let right_is_literal = bare_literal(right).is_some();

        let (decider, other) = if left_is_literal && !right_is_literal {
            (right, left)
        } else {
            (left, right)
        };
        let (decider_term, decider_ty) = self.infer(decider)?;
        if decider_ty == Type::Int {
            let other_term = self.check(other, &Type::Int)?;
            let (left_term, right_term) = if std::ptr::eq(decider, left) {
                (decider_term, other_term)
            } else {
                (other_term, decider_term)
            };
            return Ok(self.build_int_binary(op, left_term, right_term));
        }

        // The `Nat` path: the decider was inferred rather than checked, so
        // compare its type here the way `check`'s default arm would have.
        if !self.types_equal(&decider_ty, &Type::Nat) {
            return Err(self.type_mismatch_error(decider.span(), Type::Nat, decider_ty));
        }
        let other_term = self.check(other, &Type::Nat)?;
        let (left_term, right_term) = if std::ptr::eq(decider, left) {
            (decider_term, other_term)
        } else {
            (other_term, decider_term)
        };
        self.build_nat_binary(op, left_term, right_term)
    }

    /// Check a numeric expression against an expected `Int`/`Nat` (ADR
    /// 14.9.26c §2.2): the literal, negated-literal and arithmetic arms of
    /// `check`. Anything else falls through to infer-then-compare.
    pub(in crate::elaborate::exprs) fn check_numeric(
        &mut self,
        expr: &Expr,
        expected: &Type,
    ) -> ElabResult<Term> {
        match (expr, expected) {
            (Expr::IntLiteral(n, span), Type::Int) => self.int_literal(*n, false, *span),
            (Expr::Unary(UnaryOp::Neg, inner, span), Type::Int) => self.check_negated(inner, *span),
            (Expr::Binary(left, op, right, _), Type::Int) if is_arithmetic(*op) => {
                let left_term = self.check(left, &Type::Int)?;
                let right_term = self.check(right, &Type::Int)?;
                Ok(self.build_int_binary(*op, left_term, right_term).0)
            }
            (Expr::Binary(left, op, right, _), Type::Nat) if is_arithmetic(*op) => {
                let left_term = self.check(left, &Type::Nat)?;
                let right_term = self.check(right, &Type::Nat)?;
                Ok(self.build_nat_binary(*op, left_term, right_term)?.0)
            }
            // A poisoned expectation: infer for the operand's own faults and
            // pass the poison through — the root error was already reported.
            (_, Type::Error) => Ok(self.infer(expr)?.0),
            // Every other shape: infer, then compare.
            _ => {
                let (term, inferred) = self.infer(expr)?;
                if !self.types_equal(&inferred, expected) {
                    return Err(self.type_mismatch_error(expr.span(), expected.clone(), inferred));
                }
                Ok(term)
            }
        }
    }

    /// `-e` against `Int`: a bare literal folds with its sign; anything else
    /// checks against `Int` and negates.
    fn check_negated(&mut self, inner: &Expr, span: Span) -> ElabResult<Term> {
        match bare_literal(inner) {
            Some(n) => self.int_literal(n, true, span),
            None => {
                let inner_term = self.check(inner, &Type::Int)?;
                Ok(Term::int_neg(inner_term))
            }
        }
    }

    /// Elaborate polymorphic equality: infer left type, check right against it.
    fn elab_equality(&mut self, left: &Expr, op: BinOp, right: &Expr) -> ElabResult<(Term, Type)> {
        let (left_term, left_ty) = self.infer(left)?;
        let right_term = self.check(right, &left_ty)?;
        let term = self.build_equality(op, left_term, right_term, &left_ty)?;
        Ok((term, Type::Bool))
    }

    /// Elaborate pipe operator: `x |> f` ≡ `f(x)`.
    fn elab_pipe(&mut self, left: &Expr, right: &Expr, span: Span) -> ElabResult<(Term, Type)> {
        let (left_term, left_ty) = self.infer(left)?;
        let (right_term, right_ty) = self.infer(right)?;

        let Type::Arrow(param_ty, result_ty) = right_ty else {
            return Err(ElabError::expected_function(right.span(), right_ty));
        };

        if !self.types_equal(&left_ty, &param_ty) {
            return Err(ElabError::type_mismatch(span, *param_ty, left_ty));
        }

        Ok((Term::app(right_term, left_term), *result_ty))
    }

    /// Build equality check using type-specific primitives.
    fn build_equality(
        &mut self,
        op: BinOp,
        left: Term,
        right: Term,
        ty: &Type,
    ) -> ElabResult<Term> {
        // Build the equality check based on the type
        let eq_term = match ty {
            Type::String => {
                // Use the StrEq primitive
                Term::str_eq(left, right)
            }
            Type::Nat => Self::build_nat_equality(left, right),
            Type::Int => Self::build_int_equality(left, right),
            Type::Bool => {
                // Bool equality: (a && b) || (!a && !b)
                // = if a then b else !b
                let not_right = Term::if_then_else(right.clone(), Term::False, Term::True);
                Term::if_then_else(left, right, not_right)
            }
            _ => {
                // Other types don't have equality primitives yet
                Term::Sorry
            }
        };

        // Handle != by negating
        match op {
            BinOp::Eq => Ok(eq_term),
            BinOp::Ne => Ok(Term::if_then_else(eq_term, Term::False, Term::True)),
            _ => unreachable!(),
        }
    }

    /// Build boolean operation.
    fn build_bool_binop(&mut self, op: BinOp, left: Term, right: Term) -> ElabResult<Term> {
        match op {
            BinOp::And => {
                // and(a, b) = if a then b else false
                Ok(Term::if_then_else(left, right, Term::False))
            }
            BinOp::Or => {
                // or(a, b) = if a then true else b
                Ok(Term::if_then_else(left, Term::True, right))
            }
            _ => unreachable!(),
        }
    }

    /// Elaborate unary operation.
    pub(in crate::elaborate::exprs) fn elab_unary(
        &mut self,
        op: UnaryOp,
        operand: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        match op {
            UnaryOp::Not => {
                let term = self.check(operand, &Type::Bool)?;
                // not(b) = if b then false else true
                Ok((
                    Term::if_then_else(term, Term::False, Term::True),
                    Type::Bool,
                ))
            }
            UnaryOp::Neg => self.elab_negation(operand, span),
        }
    }
}
