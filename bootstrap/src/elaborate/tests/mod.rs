//! Tests for the elaborator.
//!
//! Tests are organized into separate modules by category:
//! - `items/functions` — basic function and polymorphic/generic tests
//! - `items/proofs` — theorems, axioms, have/show, blocks
//! - `items/type_defs` — type definitions, ADTs, constructor inference
//! - `items/records` — record types, field access, spread operator
//! - `expressions` — let, if, lambda, binop, strings, tuples, annotations
//! - `errors` — error case tests
//! - `visibility` — basic item visibility tests
//! - `visibility_export` — export validation / public item leak detection
//! - `cross_module_generics` — ADR 31: cross-module generic type resolution

mod cross_module_generics;
mod errors;
mod expressions;
mod items;
mod visibility;
mod visibility_export;

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

/// Helper to elaborate with a specific ElabMode.
pub(super) fn elab_with_mode(
    source: &str,
    mode: super::ElabMode,
) -> Result<Vec<CoreDef>, Vec<ElabError>> {
    let (ast, parse_errors) = parse(source);
    if !parse_errors.is_empty() {
        panic!("Parse errors: {:?}", parse_errors);
    }
    let ctx = Box::leak(Box::new(Context::new()));
    let mut elaborator = Elaborator::new(ctx);
    elaborator.elab_mode = mode;
    match elaborator.elaborate_file(&ast) {
        Ok(defs) => Ok(defs),
        Err(errors) => Err(errors),
    }
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

/// Helper to elaborate and collect warnings (expects success).
pub(super) fn elab_ok_with_warnings(source: &str) -> (Vec<CoreDef>, Vec<ElabError>) {
    let (ast, parse_errors) = parse(source);
    if !parse_errors.is_empty() {
        panic!("Parse errors: {:?}", parse_errors);
    }
    let ctx = Box::leak(Box::new(Context::new()));
    let mut elaborator = Elaborator::new(ctx);
    match elaborator.elaborate_file(&ast) {
        Ok(defs) => (defs, std::mem::take(&mut elaborator.warnings)),
        Err(errors) => panic!("Elaboration errors: {:?}", errors),
    }
}
