//! Test discovery: which `test_*` definitions are runnable, and what to do
//! when none is.
//!
//! Split out of `test_runner/mod.rs` for the 400-LOC file limit, along the
//! seam that means something: everything here decides *what to run* from the
//! elaborated definitions alone. It touches no evaluator, no filesystem and no
//! stdout, which is why `classify_empty_suite` is assertable over literal
//! counts rather than over a run.
//!
//! Tests: `bootstrap/src/test_runner/tests.rs` — the `discover_*` and
//! `require_tests_*` cases stayed with the parent when this module split out.

use tungsten_core::Type;

use tungsten_bootstrap::elaborate::CoreDef;

use super::TestFunction;

/// Discovery error for non-conforming test_* functions.
#[derive(Debug)]
pub(super) struct DiscoveryError {
    pub(super) name: String,
    pub(super) reason: String,
}

/// Check if a type represents `Unit` (arity-0, returns Unit).
fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}

/// Check if a type is an arrow (function) type.
fn is_arrow_type(ty: &Type) -> bool {
    matches!(ty, Type::Arrow(_, _))
}

/// Discover test functions from elaborated definitions.
///
/// Returns (valid tests, discovery errors).
pub(super) fn discover_tests(
    defs: &[CoreDef],
    filter: Option<&str>,
) -> (Vec<TestFunction>, Vec<DiscoveryError>) {
    let mut tests = Vec::new();
    let mut errors = Vec::new();

    for def in defs {
        if !def.name.starts_with("test_") {
            continue;
        }

        // Apply filter
        if let Some(pattern) = filter {
            if !def.name.contains(pattern) {
                continue;
            }
        }

        // Validate: must be arity 0 (not an arrow type) and return Unit
        if is_arrow_type(&def.ty) {
            errors.push(DiscoveryError {
                name: def.name.clone(),
                reason: "test function must take no parameters".to_string(),
            });
            continue;
        }

        if !is_unit_type(&def.ty) {
            errors.push(DiscoveryError {
                name: def.name.clone(),
                reason: format!("test function must return Unit, found {}", def.ty),
            });
            continue;
        }

        tests.push(TestFunction {
            name: def.name.clone(),
        });
    }

    (tests, errors)
}
/// What to do when discovery yields tests (or fails to) — pure + testable
/// (ADR 2.7.26b T5b: an empty suite must not pass vacuously under
/// `--require-tests`).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum EmptySuiteAction {
    /// `--require-tests` with zero runnable tests: fail loudly.
    FailRequireTests,
    /// Nothing discovered and nothing skipped: report "no tests found", exit 0.
    ReportNoTests,
    /// Runnable tests exist (or skips should still be reported): run them.
    Proceed,
}

pub(super) fn classify_empty_suite(
    num_tests: usize,
    num_discovery_errors: usize,
    require_tests: bool,
) -> EmptySuiteAction {
    if num_tests > 0 {
        return EmptySuiteAction::Proceed;
    }
    if require_tests {
        // Zero RUNNABLE tests — skipped-with-warning functions do not count.
        return EmptySuiteAction::FailRequireTests;
    }
    if num_discovery_errors == 0 {
        return EmptySuiteAction::ReportNoTests;
    }
    EmptySuiteAction::Proceed
}
