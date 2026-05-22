//! Display helpers for elaboration errors.
//!
//! Provides concise diagnostic label messages for inline source display.

use super::{ElabError, ElabErrorKind};

impl ElabError {
    /// Get a concise message for the primary label in diagnostic output.
    ///
    /// This is shown inline with the source code and should be brief.
    pub fn primary_label_message(&self) -> String {
        match &self.kind {
            ElabErrorKind::TypeMismatch { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }
            ElabErrorKind::ExpectedFunction(found) => {
                format!("expected function, found `{}`", found)
            }
            ElabErrorKind::ExpectedType { expected, found } => {
                format!("expected {}, found `{}`", expected, found)
            }
            ElabErrorKind::UndefinedVariable(_) => "not found in this scope".to_string(),
            ElabErrorKind::UndefinedType(_) => "not found in this scope".to_string(),
            ElabErrorKind::UndefinedConstructor(_) => "not found in this scope".to_string(),
            ElabErrorKind::ArityMismatch { expected, found: _ } => {
                format!(
                    "expected {} argument{}",
                    expected,
                    if *expected == 1 { "" } else { "s" }
                )
            }
            ElabErrorKind::CannotInferType => "type cannot be inferred".to_string(),
            ElabErrorKind::CannotInferTypeArg(var) => {
                format!("cannot infer `{}`", var)
            }
            _ => {
                // For other errors, the main message is sufficient
                String::new()
            }
        }
    }
}
