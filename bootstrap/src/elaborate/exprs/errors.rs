//! Error helpers for expression elaboration.
//!
//! Provides context-aware error construction with suggestions.

use crate::span::Span;
use tungsten_core::Type;

use crate::elaborate::error::ElabError;
use crate::elaborate::Elaborator;

use crate::utils::find_best_suggestion;

impl<'a> Elaborator<'a> {
    /// Create a type mismatch error with current context (if any).
    pub(super) fn type_mismatch_error(&self, span: Span, expected: Type, found: Type) -> ElabError {
        let err = ElabError::type_mismatch(span, expected, found);
        if let Some(ctx) = self.current_context() {
            err.with_context(ctx.clone())
        } else {
            err
        }
    }

    /// Create an undefined variable error with "did you mean" suggestion.
    pub(super) fn undefined_variable_error(&self, span: Span, name: &str) -> ElabError {
        let err = ElabError::undefined_variable(span, name);

        // Try to find a similar name to suggest
        if let Some(suggestion) = find_best_suggestion(name, self.env.all_value_names()) {
            err.with_help(format!("did you mean `{}`?", suggestion))
        } else {
            err
        }
    }

    /// Create an undefined type error with "did you mean" suggestion.
    pub(crate) fn undefined_type_error(&self, span: Span, name: &str) -> ElabError {
        let err = ElabError::undefined_type(span, name);

        // Try to find a similar name to suggest
        if let Some(suggestion) = find_best_suggestion(name, self.env.all_type_names()) {
            err.with_help(format!("did you mean `{}`?", suggestion))
        } else {
            err
        }
    }

    /// Create an undefined constructor error with "did you mean" suggestion.
    pub(crate) fn undefined_constructor_error(&self, span: Span, name: &str) -> ElabError {
        let err = ElabError::undefined_constructor(span, name);

        // Try to find a similar constructor name to suggest
        if let Some(suggestion) = find_best_suggestion(name, self.env.all_constructor_names()) {
            err.with_help(format!("did you mean `{}`?", suggestion))
        } else {
            err
        }
    }
}
