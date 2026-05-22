//! Type Checker
//!
//! Implements the typing judgment Γ ⊢ t : τ for the Tungsten core calculus.
//!
//! The type checker synthesizes types from terms (bidirectional type checking).
//!
//! ## Overview
//!
//! This module provides:
//!
//! - [`type_of`]: Synthesize the type of a term given a context
//! - [`check`]: Check a term against an expected type
//! - [`types_equal`]: Check if two types are definitionally equal (α-equivalent)
//! - [`check_type_wf`]: Verify a type is well-formed under a context
//!
//! ## Typing Rules
//!
//! The type checker implements standard typing rules for:
//!
//! - Lambda calculus (λ, application, let)
//! - Booleans and conditionals
//! - Natural numbers with recursion (natrec, natind)
//! - Products (pairs) and sums (tagged unions)
//! - Polymorphism (∀, type application)
//! - Propositional equality (refl, subst)
//! - Strings (Phase 2A)
//! - General recursion via fix (Phase 2A)
//! - Recursive types μα.τ (Phase 2A)
//! - Nat comparisons and string operations (Phase 3-Prep)
//! - Mutable references (Phase 3-Prep)

mod error;
mod rules;
mod rules_core;
mod rules_ext;
mod rules_sum;

// Re-export public API
pub use error::TypeError;
pub use rules::{check, check_type_wf, type_of, types_equal};

/// Result type for type checking
pub type TypeResult<T> = Result<T, TypeError>;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
