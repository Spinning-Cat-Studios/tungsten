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
//! - [`lambda`] - Lambda elaboration and nat_literal
//! - [`application`] - Function application and type inference
//! - [`operators`] - Binary and unary operators
//! - [`builtins`] - Built-in operations (ref cells, strings)
//! - [`blocks`] - Let, block, and statement elaboration
//! - [`proofs`] - Proof constructs (have, show, assume)
//! - [`tuples`] - Tuple and type application elaboration
//! - [`records`] - Record literal and field access elaboration
//! - [`match_expr`] - Match expression entry point
//! - [`adt_match`] - ADT/constructor pattern matching
//! - [`patterns`] - Nested pattern elaboration
//! - [`constructors`] - Constructor elaboration and injection
//! - [`helpers`] - Type utilities and helper functions
//! - [`errors`] - Error helper methods

mod adt_match;
mod application;
mod blocks;
mod builtins;
mod check_infer;
mod constructors;
mod errors;
mod helpers;
mod lambda;
mod match_expr;
mod operators;
mod patterns;
mod proofs;
mod records;
mod tuples;
