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
mod types;

pub use codegen::{CodeGen, CodeGenError};
pub use types::{AdtDef, CodegenConstructor, TypeLowering};

// Re-export inkwell for consumers
pub use inkwell;
