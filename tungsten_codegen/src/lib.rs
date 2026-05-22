// Clippy lint policy — see ADR 18.5.26h for triage decisions.
#![allow(unknown_lints)]
// Reason: devcontainer (1.95) and host (1.91) have different lint names
// --- Keep: docs deferred to v2.0 ---
#![allow(clippy::missing_errors_doc)] // Reason: docs deferred to v2.0 (8 instances)
#![allow(clippy::missing_panics_doc)] // Reason: docs deferred to v2.0
// --- Keep: permanent ---
#![allow(clippy::too_many_lines)] // Reason: governed by project check-complexity tooling
#![allow(clippy::type_complexity)]
// Reason: LLVM builder types are inherently complex
// --- Keep: style preference (widespread, fixing adds noise) ---
#![allow(clippy::manual_let_else)] // Reason: 12 instances; existing match-based let is idiomatic
#![allow(clippy::similar_names)] // Reason: LLVM binding names (ctx/cxt, val/var) are standard
#![allow(clippy::match_same_arms)] // Reason: intentionally separate arms for clarity (8 instances)
#![allow(clippy::unused_self)] // Reason: 7 instances; methods use &self for API consistency
#![allow(clippy::collapsible_match)] // Reason: nested matches are clearer in codegen dispatch
#![allow(clippy::unnecessary_wraps)] // Reason: Result return for API consistency with fallible siblings
#![allow(clippy::trivially_copy_pass_by_ref)] // Reason: &bool/&u32 params for API uniformity
#![allow(clippy::needless_pass_by_value)] // Reason: changing to &T would ripple through call sites
// --- Keep: LLVM FFI boundary ---
#![allow(clippy::cast_possible_truncation)] // Reason: LLVM API uses u32 for indices; sizes are bounded
#![allow(clippy::ptr_cast_constness)] // Reason: LLVM pointer casts between const/mut
#![allow(clippy::cast_lossless)] // Reason: u32→u64 casts in LLVM size computations
#![allow(clippy::manual_checked_ops)] // Reason: explicit overflow checks are clearer in codegen
// --- Keep: recursive helpers ---
#![allow(clippy::self_only_used_in_recursion)] // Reason: recursive type traversal methods
#![allow(clippy::only_used_in_recursion)] // Reason: same lint, older clippy name
// --- Rustc warnings: active development ---
#![allow(dead_code)] // Reason: active development — infrastructure for future use
#![allow(function_casts_as_integer)] // Reason: LLVM function pointer arithmetic

//! LLVM Code Generation for Tungsten
//!
//! This crate generates LLVM IR from Tungsten Core terms, enabling
//! compilation to native executables.
//!
//! # Architecture
//!
//! ```text
//! Core Term
//!     │
//!     ▼
//! ┌────────────────┐
//! │ Free Variable  │  → Collect free vars for closures
//! │   Analysis     │
//! └────────────────┘
//!     │
//!     ▼
//! ┌────────────────┐
//! │ Type Lowering  │  → Map Core types to LLVM types
//! └────────────────┘
//!     │
//!     ▼
//! ┌────────────────┐
//! │   Codegen      │  → Generate LLVM IR
//! └────────────────┘
//!     │
//!     ▼
//! ┌────────────────┐
//! │ LLVM Backend   │  → Object file / Executable
//! └────────────────┘
//! ```
//!
//! # Type Mapping
//!
//! | Tungsten Type | LLVM Type                    |
//! |---------------|------------------------------|
//! | Bool          | i1                           |
//! | Nat           | i64                          |
//! | Unit          | {}                           |
//! | String        | { i8*, i64 }                 |
//! | τ₁ → τ₂       | { fn_ptr, env_ptr }          |
//! | τ₁ × τ₂       | { τ₁_llvm, τ₂_llvm }         |
//! | τ₁ + τ₂       | { i8 tag, union { τ₁, τ₂ } } |
//! | ∀α. τ         | (erased, same as τ)          |
//! | μα. τ         | opaque pointer               |

mod analysis;
mod codegen;
pub mod escape_analysis;
mod types;

pub use codegen::{CodeGen, CodeGenError, SymbolEntry};
pub use types::{AdtDef, CodegenConstructor, TypeLowering};

// Re-export inkwell for consumers
pub use inkwell;
