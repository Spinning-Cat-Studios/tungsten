// Clippy lint policy — see ADR 18.5.26h for triage decisions.
#![allow(unknown_lints)] // Reason: devcontainer (1.95) and host (1.91) have different lint names
#![allow(
    // --- Keep: docs deferred to v2.0 ---
    clippy::doc_markdown,            // Reason: docs deferred to v2.0
    clippy::missing_safety_doc,      // Reason: FFI functions — docs deferred to v2.0
    clippy::missing_panics_doc,      // Reason: docs deferred to v2.0
    clippy::missing_errors_doc,      // Reason: docs deferred to v2.0
    // --- Keep: permanent ---
    clippy::result_large_err,        // Reason: TypeError is intentionally large; boxing hurts ergonomics
    clippy::too_many_lines,          // Reason: governed by project check-complexity tooling
    // --- Keep: FFI boundary (pervasive in this crate) ---
    clippy::not_unsafe_ptr_arg_deref, // Reason: extern "C" FFI functions — marking unsafe changes ABI
    clippy::cast_possible_truncation, // Reason: FFI casts for 64-bit targets, checked at call sites
    clippy::cast_sign_loss,          // Reason: FFI casts, checked at call site
    clippy::cast_possible_wrap,      // Reason: FFI casts, checked at call site
    clippy::cast_precision_loss,     // Reason: FFI casts, intentional f64 conversions
    clippy::ptr_cast_constness,      // Reason: FFI pointer casts between const/mut
    clippy::cast_lossless,           // Reason: FFI casts, u32→u64 in size computations
    clippy::manual_checked_ops,      // Reason: explicit overflow checks are clearer in eval
    // --- Keep: style preference (widespread, fixing adds noise) ---
    clippy::manual_let_else,         // Reason: 34 instances; existing match-based let is idiomatic in this crate
    clippy::match_same_arms,         // Reason: intentionally separate arms for clarity in type equality
    clippy::similar_names,           // Reason: FFI names (argc/argv, ptr/len pairs) are standard
    clippy::map_unwrap_or,           // Reason: map().unwrap_or_else() is more explicit for FFI error paths
    clippy::type_complexity,         // Reason: substitution functions have inherently complex type signatures
    clippy::return_self_not_must_use, // Reason: adding #[must_use] changes public API — defer to v2.0
    clippy::borrowed_box,            // Reason: Box<Term> used for recursive AST types; &Box is deliberate
    unused_braces,                   // Reason: macro-generated braces in with_arena! + matches!
)]

//! # Tungsten Core Calculus
//!
//! This crate implements the Phase 1 core calculus for the Tungsten proof language.
//!
//! ## Overview
//!
//! The core calculus is a simply-typed lambda calculus with:
//! - Base types: `Bool`, `Nat`, `Unit`, `Void`, `Prop`
//! - Type constructors: `→`, `×`, `+`, `∀α`, `Eq τ t t`
//! - Polymorphism: rank-1 (∀α. τ)
//! - Propositional equality: `Eq τ t₁ t₂` with `refl` and `subst`
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    TUNGSTEN CORE                            │
//! ├─────────────────────────────────────────────────────────────┤
//! │  types.rs    - Type syntax (τ)                              │
//! │  terms.rs    - Term syntax (t)                              │
//! │  context.rs  - Typing contexts (Γ)                          │
//! │  typecheck.rs - Typing judgment (Γ ⊢ t : τ)                 │
//! │  eval.rs     - Evaluation (t → t')                          │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use tungsten_core::prelude::*;
//!
//! // Build a term: (λx:Nat. x) zero
//! let id = Term::lambda("x", Type::Nat, Term::var("x"));
//! let term = Term::app(id, Term::Zero);
//!
//! // Type check
//! let ctx = Context::new();
//! let ty = type_of(&ctx, &term).unwrap();
//! assert_eq!(ty, Type::Nat);
//!
//! // Evaluate
//! let result = eval(&term);
//! assert_eq!(result, Term::Zero);
//! ```
//!
//! ## Phase 1 Specification
//!
//! ### Types
//!
//! ```text
//! τ ::= Bool
//!     | Nat
//!     | Unit
//!     | Void
//!     | Prop
//!     | τ → τ
//!     | τ × τ
//!     | τ + τ
//!     | α
//!     | ∀α. τ
//!     | Eq τ t t
//! ```
//!
//! ### Terms
//!
//! ```text
//! t ::= x                                      -- variable
//!     | λx:τ. t                                -- lambda
//!     | t t                                    -- application
//!     | let x : τ = t in t                     -- let binding
//!     | true | false                           -- booleans
//!     | if t then t else t                     -- conditional
//!     | ()                                     -- unit
//!     | absurd [τ] t                           -- void elimination
//!     | zero                                   -- nat zero
//!     | succ t                                 -- nat successor
//!     | natrec [τ] t t t                       -- nat recursion
//!     | natind [P] t t t                       -- nat induction
//!     | (t, t)                                 -- pair
//!     | fst t                                  -- first projection
//!     | snd t                                  -- second projection
//!     | inl [τ + τ] t                          -- left injection
//!     | inr [τ + τ] t                          -- right injection
//!     | case t of inl x => t | inr y => t      -- case analysis
//!     | Λα. t                                  -- type abstraction
//!     | t [τ]                                  -- type application
//!     | refl [τ] t                             -- equality introduction
//!     | subst [τ] [P] t t                      -- equality elimination
//!     | (t : τ)                                -- type annotation
//!     | sorry                                  -- unsafe axiom
//! ```

pub mod context;
pub mod eval;
pub mod ffi;
pub mod terms;
pub mod typecheck;
pub mod types;

// Force-link the runtime crate so its #[no_mangle] symbols are
// exported from our cdylib (signal handler, diagnostics).
extern crate tungsten_runtime;

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::context::Context;
    pub use crate::eval::{eval, eval_with_limit, step, StepResult};
    pub use crate::terms::Term;
    pub use crate::terms::{SpannedTerm, TermSpan};
    pub use crate::typecheck::{check, check_type_wf, type_of, TypeError, TypeResult};
    pub use crate::types::Type;
}

// Re-export main types at crate root
pub use context::Context;
pub use eval::{eval, step};
pub use terms::Term;
pub use terms::{SpannedTerm, TermSpan};
pub use typecheck::{type_of, types_equal, TypeError};
pub use types::{types_equal_alpha, Type};

#[cfg(test)]
mod integration_tests;
