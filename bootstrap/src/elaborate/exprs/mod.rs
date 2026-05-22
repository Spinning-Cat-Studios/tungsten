//! Expression elaboration: Surface Expr → Core Term
//!
//! Implements bidirectional type checking:
//! - `check(expr, expected)` - check expression against known type
//! - `infer(expr)` - synthesize type from expression
//!
//! This approach gives better error messages and requires fewer annotations
//! than full Hindley-Milner inference.
//!
//! ## Module Structure
//!
//! The elaboration logic is split across several modules:
//! - [`check_infer`] - Main bidirectional check/infer entry points
//! - [`application`] - Function application and type inference
//! - [`blocks`] - Let, block, and statement elaboration
//! - [`match_expr`] - Match expression entry point
//! - [`adt_match`] - ADT/constructor pattern matching
//! - [`patterns`] - Nested pattern elaboration
//! - [`constructors`] - Constructor elaboration and injection
//! - [`helpers`] - Type utilities and helper functions
//! - [`errors`] - Error helper methods
//! - [`forms`] - Expression form elaborators (lambda, operators, builtins,
//!   proofs, records, tuples, type_args)

mod adt_match;
mod application;
mod blocks;
mod blocks_tuples;
mod check_infer;
mod constructors;
mod errors;
mod forms;
mod helpers;
mod if_let;
mod let_else;
mod match_expr;
mod patterns;
mod try_block;
