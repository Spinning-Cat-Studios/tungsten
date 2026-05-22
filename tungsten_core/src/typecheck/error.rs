//! Type checking errors
//!
//! This module defines the error types returned by the type checker.

use std::fmt;

use crate::types::Type;

/// Type checking errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// Variable not found in context
    UnboundVariable(String),

    /// Type variable not in scope
    UnboundTypeVar(String),

    /// Expected a function type, got something else
    NotAFunction { got: Type },

    /// Type mismatch in application
    ArgumentTypeMismatch { expected: Type, got: Type },

    /// Expected a product type for fst/snd
    NotAProduct { got: Type },

    /// Expected a sum type for case
    NotASum { got: Type },

    /// Branches of if/case have different types
    BranchTypeMismatch { then_type: Type, else_type: Type },

    /// Expected Bool for if condition
    ConditionNotBool { got: Type },

    /// Expected Nat for natrec/natind/succ
    NotANat { got: Type },

    /// Expected a forall type for type application
    NotPolymorphic { got: Type },

    /// Expected Void for absurd
    NotVoid { got: Type },

    /// Expected an equality type for subst
    NotEquality { got: Type },

    /// General type mismatch
    TypeMismatch { expected: Type, got: Type },

    /// Type is not well-formed
    MalformedType(Type),

    /// Natrec type mismatch: succ case doesn't have right type
    NatRecSuccTypeMismatch {
        expected: Type, // Nat → τ → τ
        got: Type,
    },

    /// `NatInd` motive mismatch
    NatIndMotiveMismatch { expected: Type, got: Type },

    /// Motive is not a function from τ to Prop
    MotiveNotFunction { got: Type },

    // === Phase 2B: Flat ADT Errors (ADR 2.2.26) ===
    /// Invalid variant index in ADT construct/match
    InvalidVariantIndex { index: usize, num_variants: usize },

    /// Expected an ADT type
    NotAnAdt { got: Type },

    /// Match expression has no arms
    EmptyMatch,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // "Expected X, got: {got}" family
        if let Some((expected_desc, got)) = self.expected_got_msg() {
            return write!(f, "Expected {expected_desc}, got: {got}");
        }
        // "X mismatch: expected {e}, got {g}" family
        if let Some((label, expected, got)) = self.mismatch_msg() {
            return write!(f, "{label} mismatch: expected {expected}, got {got}");
        }
        match self {
            TypeError::UnboundVariable(v) => write!(f, "Unbound variable: {v}"),
            TypeError::UnboundTypeVar(v) => write!(f, "Unbound type variable: {v}"),
            TypeError::BranchTypeMismatch {
                then_type,
                else_type,
            } => {
                write!(f, "Branch type mismatch: {then_type} vs {else_type}")
            }
            TypeError::ConditionNotBool { got } => {
                write!(f, "Condition must be Bool, got: {got}")
            }
            TypeError::MalformedType(ty) => write!(f, "Malformed type: {ty}"),
            TypeError::MotiveNotFunction { got } => {
                write!(f, "Motive must be a function type, got: {got}")
            }
            TypeError::InvalidVariantIndex {
                index,
                num_variants,
            } => {
                write!(
                    f,
                    "Invalid variant index {index} for ADT with {num_variants} variants"
                )
            }
            TypeError::EmptyMatch => {
                write!(f, "Match expression has no arms")
            }
            // Handled by helpers above
            TypeError::NotAFunction { .. }
            | TypeError::NotAProduct { .. }
            | TypeError::NotASum { .. }
            | TypeError::NotANat { .. }
            | TypeError::NotPolymorphic { .. }
            | TypeError::NotVoid { .. }
            | TypeError::NotEquality { .. }
            | TypeError::NotAnAdt { .. }
            | TypeError::ArgumentTypeMismatch { .. }
            | TypeError::TypeMismatch { .. }
            | TypeError::NatRecSuccTypeMismatch { .. }
            | TypeError::NatIndMotiveMismatch { .. } => unreachable!(),
        }
    }
}

impl TypeError {
    /// "Expected X, got: {got}" — single-type-mismatch errors.
    fn expected_got_msg(&self) -> Option<(&str, &Type)> {
        match self {
            TypeError::NotAFunction { got } => Some(("function type", got)),
            TypeError::NotAProduct { got } => Some(("product type", got)),
            TypeError::NotASum { got } => Some(("sum type", got)),
            TypeError::NotANat { got } => Some(("Nat", got)),
            TypeError::NotPolymorphic { got } => Some(("polymorphic type (∀α. τ)", got)),
            TypeError::NotVoid { got } => Some(("Void", got)),
            TypeError::NotEquality { got } => Some(("Eq type", got)),
            TypeError::NotAnAdt { got } => Some(("ADT type", got)),
            _ => None,
        }
    }

    /// "X mismatch: expected {e}, got {g}" — two-type-mismatch errors.
    fn mismatch_msg(&self) -> Option<(&str, &Type, &Type)> {
        match self {
            TypeError::ArgumentTypeMismatch { expected, got } => {
                Some(("Argument type", expected, got))
            }
            TypeError::TypeMismatch { expected, got } => Some(("Type", expected, got)),
            TypeError::NatRecSuccTypeMismatch { expected, got } => {
                Some(("natrec succ case type", expected, got))
            }
            TypeError::NatIndMotiveMismatch { expected, got } => {
                Some(("natind motive", expected, got))
            }
            _ => None,
        }
    }
}

impl std::error::Error for TypeError {}
