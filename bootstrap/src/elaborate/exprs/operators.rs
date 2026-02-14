//! Operator elaboration.
//!
//! Handles:
//! - Binary operators (arithmetic, comparison, logical, pipe)
//! - Unary operators (not, neg)

use crate::ast::{BinOp, Expr, UnaryOp};
use crate::span::{Span, Spanned};
use tungsten_core::{Term, Type};

use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate binary operation.
    pub(super) fn elab_binary(
        &mut self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        match op {
            // Arithmetic: Nat -> Nat -> Nat
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let left_term = self.check(left, &Type::Nat)?;
                let right_term = self.check(right, &Type::Nat)?;
                // For now, use natrec to implement these
                // This is a placeholder - proper implementation would need primitives
                let term = self.build_nat_binop(op, left_term, right_term)?;
                Ok((term, Type::Nat))
            }

            // String concatenation: String -> String -> String
            BinOp::Concat => {
                let left_term = self.check(left, &Type::String)?;
                let right_term = self.check(right, &Type::String)?;
                Ok((Term::str_concat(left_term, right_term), Type::String))
            }

            // Comparison: Nat -> Nat -> Bool (simplified)
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let left_term = self.check(left, &Type::Nat)?;
                let right_term = self.check(right, &Type::Nat)?;
                let term = self.build_nat_comparison(op, left_term, right_term)?;
                Ok((term, Type::Bool))
            }

            // Equality: polymorphic, but we infer from left side
            BinOp::Eq | BinOp::Ne => {
                let (left_term, left_ty) = self.infer(left)?;
                let right_term = self.check(right, &left_ty)?;
                let term = self.build_equality(op, left_term, right_term, &left_ty)?;
                Ok((term, Type::Bool))
            }

            // Logical: Bool -> Bool -> Bool
            BinOp::And | BinOp::Or => {
                let left_term = self.check(left, &Type::Bool)?;
                let right_term = self.check(right, &Type::Bool)?;
                let term = self.build_bool_binop(op, left_term, right_term)?;
                Ok((term, Type::Bool))
            }

            // Pipe: f |> g = g(f)
            BinOp::Pipe => {
                // x |> f  ≡  f(x)
                // This is just application with arguments swapped
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
            Type::Nat => {
                // Nat equality: a == b iff (a <= b) && (b <= a)
                // Implemented as: if (a <= b) then (b <= a) else false
                Term::if_then_else(
                    Term::nat_le(left.clone(), right.clone()),
                    Term::nat_le(right, left),
                    Term::False,
                )
            }
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
    pub(super) fn elab_unary(
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
            UnaryOp::Neg => {
                // Negation not directly supported for Nat
                Err(ElabError::unsupported(span, "numeric negation")
                    .with_help("Nat has no negative numbers"))
            }
        }
    }
}
