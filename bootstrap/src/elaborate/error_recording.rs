//! Error recording and diagnostic methods for the Elaborator.

use crate::span::Span;

use super::error::{ElabError, ElabErrorKind, ExpectedContext};
use super::Elaborator;

use crate::config::MAX_CONTEXT_DEPTH;

impl<'a> Elaborator<'a> {
    /// Push a type expectation context onto the stack.
    pub fn push_context(&mut self, context: ExpectedContext) {
        if self.context_stack.len() < MAX_CONTEXT_DEPTH {
            self.context_stack.push(context);
        }
    }

    /// Pop the top context from the stack.
    pub fn pop_context(&mut self) {
        self.context_stack.pop();
    }

    /// Get the current type expectation context, if any.
    pub fn current_context(&self) -> Option<&ExpectedContext> {
        self.context_stack.last()
    }

    /// Get the file path for the current module (for error reporting).
    pub fn get_current_file(&self) -> Option<std::path::PathBuf> {
        self.env.get_module_file(&self.current_module).cloned()
    }

    /// Add file path to an error based on the current module.
    pub fn error_with_file(&self, mut error: ElabError) -> ElabError {
        if let Some(file_path) = self.get_current_file() {
            error = error.with_file_path(file_path);
        }
        error
    }

    /// Record an error with file path attached.
    pub fn record_error(&mut self, error: ElabError) {
        self.errors.push(self.error_with_file(error));
    }

    /// Record a warning with file path attached.
    pub fn record_warning(&mut self, warning: ElabError) {
        self.warnings.push(self.error_with_file(warning));
    }

    /// Record an error and continue (for error recovery).
    #[allow(dead_code)]
    fn error(&mut self, span: Span, kind: ElabErrorKind) -> ElabError {
        ElabError::new(span, kind)
    }

    /// Record an error with a help message.
    #[allow(dead_code)]
    fn error_with_help(&mut self, span: Span, kind: ElabErrorKind, help: &str) -> ElabError {
        let mut err = ElabError::new(span, kind);
        err.help = Some(help.to_string());
        err
    }

    /// Record a warning (non-fatal diagnostic).
    pub fn warn(&mut self, warning: ElabError) {
        self.record_warning(warning);
    }
}
