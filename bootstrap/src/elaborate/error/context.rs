//! Context tracking for better error messages.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::span::Span;

/// Why we expect a particular type at a given location.
///
/// This context is threaded through type checking to provide
/// "expected X because of Y" explanations in error messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedContext {
    /// What created this expectation
    pub reason: ExpectedReason,
    /// Where the expectation comes from (e.g., the return type annotation)
    pub span: Span,
    /// File where the expectation originates (for cross-file diagnostics, ADR 15.5.26a).
    #[serde(default)]
    pub file_path: Option<PathBuf>,
}

impl ExpectedContext {
    /// Create a new context.
    pub fn new(reason: ExpectedReason, span: Span) -> Self {
        Self {
            reason,
            span,
            file_path: None,
        }
    }

    /// Attach a file path to this context for cross-file diagnostics.
    #[must_use]
    pub fn in_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Create context for a return type.
    pub fn return_type(span: Span) -> Self {
        Self::new(ExpectedReason::ReturnType, span)
    }

    /// Create context for a function argument.
    pub fn function_arg(position: usize, span: Span) -> Self {
        Self::new(ExpectedReason::FunctionArg { position }, span)
    }

    /// Create context for a let binding type annotation.
    pub fn let_annotation(span: Span) -> Self {
        Self::new(ExpectedReason::LetAnnotation, span)
    }

    /// Create context for an if condition.
    pub fn if_condition(span: Span) -> Self {
        Self::new(ExpectedReason::IfCondition, span)
    }

    /// Create context for branch unification.
    pub fn branch_unification(span: Span) -> Self {
        Self::new(ExpectedReason::BranchUnification, span)
    }

    /// Create context for a binary operation operand.
    pub fn binary_operand(span: Span) -> Self {
        Self::new(ExpectedReason::BinaryOperand, span)
    }

    /// Create context for a match scrutinee.
    pub fn match_scrutinee(span: Span) -> Self {
        Self::new(ExpectedReason::MatchScrutinee, span)
    }

    /// Create context for pattern matching.
    pub fn pattern_match(span: Span) -> Self {
        Self::new(ExpectedReason::PatternMatch, span)
    }

    /// Get a human-readable explanation.
    pub fn explanation(&self) -> String {
        match &self.reason {
            ExpectedReason::ReturnType => "expected due to return type".to_string(),
            ExpectedReason::FunctionArg { position } => {
                format!("expected due to argument {}", position + 1)
            }
            ExpectedReason::LetAnnotation => "expected due to type annotation".to_string(),
            ExpectedReason::IfCondition => "expected `Bool` for condition".to_string(),
            ExpectedReason::BranchUnification => "expected to match other branch".to_string(),
            ExpectedReason::BinaryOperand => "expected due to operator".to_string(),
            ExpectedReason::MatchScrutinee => "expected due to match scrutinee type".to_string(),
            ExpectedReason::PatternMatch => "expected due to pattern".to_string(),
            ExpectedReason::TypeAnnotation => "expected due to type annotation".to_string(),
            ExpectedReason::ConstructorField { name, position } => {
                if let Some(name) = name {
                    format!("expected due to field `{}`", name)
                } else {
                    format!("expected due to field {}", position + 1)
                }
            }
            ExpectedReason::Other(msg) => msg.clone(),
        }
    }
}

/// The reason for a type expectation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpectedReason {
    /// Function return type annotation
    ReturnType,
    /// Function argument at a position
    FunctionArg { position: usize },
    /// Let binding type annotation
    LetAnnotation,
    /// If condition must be Bool
    IfCondition,
    /// If/match branches must have same type
    BranchUnification,
    /// Binary operator operand
    BinaryOperand,
    /// Match scrutinee type determines pattern types
    MatchScrutinee,
    /// Pattern must match scrutinee type
    PatternMatch,
    /// Explicit type annotation
    TypeAnnotation,
    /// Constructor field
    ConstructorField {
        name: Option<String>,
        position: usize,
    },
    /// Other reason with custom message
    Other(String),
}
