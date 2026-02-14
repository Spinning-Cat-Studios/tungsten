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
        match self {
            TypeError::UnboundVariable(v) => write!(f, "Unbound variable: {v}"),
            TypeError::UnboundTypeVar(v) => write!(f, "Unbound type variable: {v}"),
            TypeError::NotAFunction { got } => write!(f, "Expected function type, got: {got}"),
            TypeError::ArgumentTypeMismatch { expected, got } => {
                write!(f, "Argument type mismatch: expected {expected}, got {got}")
            }
            TypeError::NotAProduct { got } => write!(f, "Expected product type, got: {got}"),
            TypeError::NotASum { got } => write!(f, "Expected sum type, got: {got}"),
            TypeError::BranchTypeMismatch {
                then_type,
                else_type,
            } => {
                write!(f, "Branch type mismatch: {then_type} vs {else_type}")
            }
            TypeError::ConditionNotBool { got } => {
                write!(f, "Condition must be Bool, got: {got}")
            }
            TypeError::NotANat { got } => write!(f, "Expected Nat, got: {got}"),
            TypeError::NotPolymorphic { got } => {
                write!(f, "Expected polymorphic type (∀α. τ), got: {got}")
            }
            TypeError::NotVoid { got } => write!(f, "Expected Void, got: {got}"),
            TypeError::NotEquality { got } => write!(f, "Expected Eq type, got: {got}"),
            TypeError::TypeMismatch { expected, got } => {
                write!(f, "Type mismatch: expected {expected}, got {got}")
            }
            TypeError::MalformedType(ty) => write!(f, "Malformed type: {ty}"),
            TypeError::NatRecSuccTypeMismatch { expected, got } => {
                write!(
                    f,
                    "natrec succ case type mismatch: expected {expected}, got {got}"
                )
            }
            TypeError::NatIndMotiveMismatch { expected, got } => {
                write!(f, "natind motive mismatch: expected {expected}, got {got}")
            }
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
            TypeError::NotAnAdt { got } => {
                write!(f, "Expected ADT type, got: {got}")
            }
            TypeError::EmptyMatch => {
                write!(f, "Match expression has no arms")
            }
        }
    }
}

impl std::error::Error for TypeError {}
