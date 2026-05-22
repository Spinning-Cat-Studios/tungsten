//! Convenience constructors for common elaboration errors.
//!
//! These factory methods provide a concise API for creating errors with
//! appropriate error kinds and default messages.
//!
//! Module-system errors (imports, visibility) are in `constructors_modules.rs`.

use crate::span::Span;
use tungsten_core::{Term, Type};

use super::{ElabError, ElabErrorKind};

impl ElabError {
    /// Create an "undefined variable" error.
    pub fn undefined_variable(span: Span, name: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UndefinedVariable(name.into()))
    }

    /// Create an "undefined type" error.
    pub fn undefined_type(span: Span, name: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UndefinedType(name.into()))
    }

    /// Create an "undefined constructor" error.
    pub fn undefined_constructor(span: Span, name: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UndefinedConstructor(name.into()))
    }

    /// Create a "type mismatch" error.
    pub fn type_mismatch(span: Span, expected: Type, found: Type) -> Self {
        Self::new(span, ElabErrorKind::TypeMismatch { expected, found })
    }

    /// Create a "cannot infer type" error.
    pub fn cannot_infer(span: Span) -> Self {
        Self::new(span, ElabErrorKind::CannotInferType)
    }

    /// Create an "arity mismatch" error.
    pub fn arity_mismatch(span: Span, expected: usize, found: usize) -> Self {
        Self::new(span, ElabErrorKind::ArityMismatch { expected, found })
    }

    /// Create an "expected function" error.
    pub fn expected_function(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::ExpectedFunction(found))
    }

    /// Create an "unsupported feature" error.
    pub fn unsupported(span: Span, feature: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UnsupportedFeature(feature.into()))
    }

    /// Create a "duplicate definition" error.
    pub fn duplicate(span: Span, name: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::DuplicateDefinition(name.into()))
    }

    /// Create a "no main function" error.
    pub fn no_main_function(span: Span) -> Self {
        Self::new(span, ElabErrorKind::NoMainFunction)
            .with_help("add a function like `fn main() -> Nat { 42 }`")
    }

    /// Create a "contains sorry" error.
    pub fn contains_sorry(span: Span) -> Self {
        Self::new(span, ElabErrorKind::ContainsSorry)
            .with_help("replace `sorry` with an actual implementation")
    }

    /// Create a "`refl` expected equality type" error (ADR 21.5.26d).
    pub fn refl_expected_equality(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::ReflExpectedEquality(found))
    }

    /// Create an "invalid `refl`" error — sides are not definitionally equal (ADR 21.5.26d).
    pub fn invalid_refl(span: Span, left: Term, right: Term) -> Self {
        Self::new(span, ElabErrorKind::InvalidRefl { left, right })
    }

    /// Create a "`subst` expected equality" error (ADR 21.5.26d).
    pub fn subst_expected_equality(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::SubstExpectedEquality(found))
    }

    /// Create a "`trans` endpoint mismatch" error (ADR 21.5.26d).
    pub fn trans_endpoint_mismatch(span: Span, left: Term, right: Term) -> Self {
        Self::new(span, ElabErrorKind::TransEndpointMismatch { left, right })
    }

    /// Create a "`cong` expected function" error (ADR 21.5.26d).
    pub fn cong_expected_function(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::CongExpectedFunction(found))
    }

    /// Create a "motive not predicate" error (ADR 21.5.26g).
    pub fn motive_not_predicate(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::MotiveNotPredicate(found))
            .with_help("motive must be a predicate lambda: `|x: τ| <type-expr>`")
    }

    /// Create a "motive domain mismatch" error (ADR 21.5.26g).
    pub fn motive_domain_mismatch(span: Span, expected: Type, found: Type) -> Self {
        Self::new(
            span,
            ElabErrorKind::MotiveDomainMismatch { expected, found },
        )
        .with_help("motive parameter type must match the equality's base type")
    }

    /// Create a "motive body not type" error (ADR 21.5.26g).
    pub fn motive_body_not_type(span: Span) -> Self {
        Self::new(span, ElabErrorKind::MotiveBodyNotType)
            .with_help("motive body must be a type expression, not a term")
    }

    /// Create a "natind motive not Nat" error (ADR 22.5.26a).
    pub fn natind_motive_not_nat(span: Span, found: Type) -> Self {
        Self::new(span, ElabErrorKind::NatIndMotiveNotNat(found))
            .with_help("natind motive parameter type must be `Nat`")
    }

    /// Create a generic error with a custom message.
    pub fn other(span: Span, message: &str) -> Self {
        Self::new(span, ElabErrorKind::Other(message.to_string()))
    }
}
