//! Error helpers for expression elaboration.
//!
//! Provides context-aware error construction with suggestions.

use std::path::PathBuf;

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

    /// Look up the defining file and span for a global function (for cross-file diagnostics).
    ///
    /// Performs a three-step lookup:
    ///   1. `env.get_item_module(name)` → which module owns this name
    ///   2. `env.get_module_file(&module_path)` → filesystem path for that module
    ///   3. `env.lookup_value(name)` → `ValueDef` with the definition span
    ///
    /// Returns `Some((file_path, def_span))` when the function is defined in a
    /// different module from `current_module` and the file path is known.
    ///
    /// Note: `ValueDef.span` covers the entire function definition, not just
    /// the return type annotation. Narrowing to the return-type span would
    /// require storing a separate `return_type_span` in `ValueDef`.
    pub(super) fn cross_file_info_for_function(&self, name: &str) -> Option<(PathBuf, Span)> {
        let item_module = self.env.get_item_module(name)?;
        if item_module == &self.current_module {
            return None; // Same module — not a cross-file reference
        }
        let file_path = self.env.get_module_file(item_module)?.clone();
        let value_def = self.env.lookup_value(name)?;
        Some((file_path, value_def.span))
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
