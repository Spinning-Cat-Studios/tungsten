//! Tests for HintCategory mapping from ElabErrorKind variants.

use super::*;
use crate::ElabErrorKind;
use tungsten_core::Type;

#[test]
fn test_category_type_mismatch() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::TypeMismatch
    );
}

#[test]
fn test_category_expected_function() {
    let kind = ElabErrorKind::ExpectedFunction(Type::Nat);
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::TypeMismatch
    );
}

#[test]
fn test_category_undefined_variable() {
    let kind = ElabErrorKind::UndefinedVariable("x".to_string());
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::NameResolution
    );
}

#[test]
fn test_category_module_not_found() {
    let kind = ElabErrorKind::ModuleNotFound {
        module: "foo".to_string(),
        suggestion: None,
    };
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::NameResolution
    );
}

#[test]
fn test_category_non_exhaustive_match() {
    let kind = ElabErrorKind::NonExhaustiveMatch;
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::PatternMatching
    );
}

#[test]
fn test_category_no_main() {
    let kind = ElabErrorKind::NoMainFunction;
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::EntryPoint
    );
}

#[test]
fn test_category_unsupported_feature() {
    let kind = ElabErrorKind::UnsupportedFeature("traits".to_string());
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::Elaboration
    );
}

#[test]
fn test_category_other() {
    let kind = ElabErrorKind::Other("something".to_string());
    assert_eq!(HintCategory::from_error_kind(&kind), HintCategory::General);
}

#[test]
fn test_category_arity_mismatch() {
    let kind = ElabErrorKind::ArityMismatch {
        expected: 2,
        found: 3,
    };
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::TypeMismatch
    );
}

#[test]
fn test_category_cannot_infer_type() {
    let kind = ElabErrorKind::CannotInferType;
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::TypeMismatch
    );
}

#[test]
fn test_category_pattern_too_deep() {
    let kind = ElabErrorKind::PatternTooDeep { depth: 10, max: 5 };
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::PatternMatching
    );
}

#[test]
fn test_category_contains_sorry() {
    let kind = ElabErrorKind::ContainsSorry;
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::EntryPoint
    );
}

#[test]
fn test_category_duplicate_definition() {
    let kind = ElabErrorKind::DuplicateDefinition("foo".to_string());
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::NameResolution
    );
}

#[test]
fn test_category_undefined_type() {
    let kind = ElabErrorKind::UndefinedType("Foo".to_string());
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::NameResolution
    );
}

#[test]
fn test_category_dead_code_after_return() {
    let kind = ElabErrorKind::DeadCodeAfterReturn;
    assert_eq!(
        HintCategory::from_error_kind(&kind),
        HintCategory::ControlFlow
    );
}
