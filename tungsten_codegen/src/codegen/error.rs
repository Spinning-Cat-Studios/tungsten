//! Code generation error types.

/// Code generation errors.
#[derive(Debug, Clone)]
pub enum CodeGenError {
    /// Variable not found in scope.
    UnboundVariable(String),
    /// Type error during code generation.
    TypeError(String),
    /// LLVM error.
    LlvmError(String),
    /// Unsupported feature.
    Unsupported(String),
}

impl std::fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeGenError::UnboundVariable(v) => write!(f, "unbound variable: {}", v),
            CodeGenError::TypeError(msg) => write!(f, "type error: {}", msg),
            CodeGenError::LlvmError(msg) => write!(f, "LLVM error: {}", msg),
            CodeGenError::Unsupported(msg) => write!(f, "unsupported: {}", msg),
        }
    }
}

impl std::error::Error for CodeGenError {}
