//! Execution-related compilation
//!
//! Compilation of control flow and function-related constructs:
//! - `closures`: Lambda compilation and closure conversion
//! - `control`: Control flow (if, natrec)
//! - `direct_calls`: Uncurried calling convention for known-arity functions
//! - `polymorphism`: Type abstraction and monomorphization
//! - `inference`: Type inference for code generation
//! - `globals`: Global references and extern calls

pub(crate) mod closures;
pub(crate) mod control;
pub(crate) mod direct_calls;
pub(crate) mod globals;
pub(crate) mod inference;
pub(crate) mod polymorphism;
