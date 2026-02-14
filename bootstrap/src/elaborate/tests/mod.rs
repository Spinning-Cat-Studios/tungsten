//! Tests for the elaborator.
//!
//! Tests are organized into separate modules by category:
//! - `functions` — basic function and polymorphic/generic tests
//! - `expressions` — let, if, lambda, binop, strings, tuples, annotations
//! - `proofs` — theorems, axioms, have/show, blocks
//! - `type_defs` — type definitions, ADTs, constructor inference
//! - `records` — record types, field access, spread operator
//! - `errors` — error case tests
//! - `visibility` — item visibility and export validation tests
//! - `cross_module_generics` — ADR 31: cross-module generic type resolution

mod cross_module_generics;
mod errors;
mod expressions;
mod functions;
mod proofs;
mod records;
mod type_defs;
mod visibility;

use super::*;
use crate::parse;
use tungsten_core::Context;

// Re-export types used by submodule tests
#[allow(unused_imports)]
pub(super) use tungsten_core::{Term, Type};

/// Helper to elaborate source and return results.
pub(super) fn elab(source: &str) -> Result<Vec<CoreDef>, Vec<ElabError>> {
    let (ast, parse_errors) = parse(source);
    if !parse_errors.is_empty() {
        panic!("Parse errors: {:?}", parse_errors);
    }
    let ctx = Box::leak(Box::new(Context::new()));
    elaborate(&ast, ctx)
}

/// Helper to elaborate and expect success.
pub(super) fn elab_ok(source: &str) -> Vec<CoreDef> {
    match elab(source) {
        Ok(defs) => defs,
        Err(errors) => panic!("Elaboration errors: {:?}", errors),
    }
}

/// Helper to elaborate and expect failure.
pub(super) fn elab_err(source: &str) -> Vec<ElabError> {
    match elab(source) {
        Ok(_) => panic!("Expected elaboration error"),
        Err(errors) => errors,
    }
}
