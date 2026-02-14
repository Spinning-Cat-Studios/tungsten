//! Constructor elaboration.
//!
//! This module handles the elaboration of ADT constructors, including:
//! - Nullary constructor references (e.g., `Nil`, `None`)
//! - Constructor applications with arguments (e.g., `Some(x)`, `Cons(h, t)`)
//! - Type inference for constructor arguments
//! - Building injection chains for sum types
//!
//! The module is split into submodules:
//! - `context` - Constructor context lookup
//! - `helpers` - Shared helper functions
//! - `nullary` - Nullary constructor elaboration
//! - `application` - Constructor application elaboration
//! - `injection` - Sum type injection building
//! - `type_matching` - Type matching utilities

mod application;
mod context;
mod helpers;
mod injection;
mod nullary;
mod type_matching;

#[cfg(test)]
mod tests;
