//! Error types for the elaborator.
//!
//! Following Rust's error message quality standard with spans, notes, and help.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::span::Span;
use tungsten_core::Type;

// ─────────────────────────────────────────────────────────────────────────────
// Context tracking for better error messages
// ─────────────────────────────────────────────────────────────────────────────

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
}

impl ExpectedContext {
    /// Create a new context.
    pub fn new(reason: ExpectedReason, span: Span) -> Self {
        Self { reason, span }
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

use std::path::PathBuf;

/// An elaboration error with source location and diagnostic information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElabError {
    /// The primary error message
    pub message: String,
    /// Source span where the error occurred
    pub span: Span,
    /// The kind of error (for categorization)
    pub kind: ElabErrorKind,
    /// Additional notes explaining the error
    pub notes: Vec<Note>,
    /// Optional help text with suggestions
    pub help: Option<String>,
    /// Context explaining why we expected this type (for type errors)
    pub context: Option<ExpectedContext>,
    /// Optional file path where the error occurred (for multi-file diagnostics)
    #[serde(default)]
    pub file_path: Option<PathBuf>,
}

/// A note attached to an error (secondary location or explanation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// The note message
    pub message: String,
    /// Optional span for this note
    pub span: Option<Span>,
}

/// Categories of elaboration errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElabErrorKind {
    // ─────────────────────────────────────────────────────────────────────────
    // Name resolution errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Reference to undefined variable
    UndefinedVariable(String),

    /// Reference to undefined type
    UndefinedType(String),

    /// Reference to undefined constructor
    UndefinedConstructor(String),

    /// Duplicate definition
    DuplicateDefinition(String),

    /// Module not found in qualified path
    ModuleNotFound {
        module: String,
        suggestion: Option<String>,
    },

    /// Item not found in specified module
    ItemNotFoundInModule { module: String, item: String },

    /// Duplicate import (same name imported twice)
    ///
    /// The diagnostic's primary span is `second_import_span` (the import that triggered
    /// duplicate detection). The `first_import_span` is rendered as a secondary label.
    DuplicateImport {
        /// The name that was imported twice
        name: String,
        /// Span where the first import was written
        first_import_span: Span,
        /// Span where the second (duplicate) import was written
        second_import_span: Span,
        /// Module from which the name was first imported
        first_source_module: String,
        /// Module from which the name was imported again
        second_source_module: String,
    },

    /// Glob import conflict (same name imported from multiple globs)
    GlobConflict {
        /// The name that conflicts
        name: String,
        /// First module that exports this name
        first_module: String,
        /// Second module that exports this name
        second_module: String,
    },

    /// Unresolved import path
    UnresolvedImport(String),

    /// Private module accessed from outside its visibility scope
    PrivateModule {
        /// The module path that was accessed
        module_path: String,
        /// The module path from which access was attempted
        accessed_from: String,
    },

    /// Private item (type, value, or constructor) accessed from outside its visibility scope
    PrivateItem {
        /// The name of the item that was accessed
        item_name: String,
        /// The kind of item (for better error messages)
        item_kind: String,
        /// The module where the item is defined
        defined_in: String,
        /// The module from which access was attempted
        accessed_from: String,
    },

    /// Public item leaks a less visible type in its signature
    ///
    /// Example: `pub fn foo() -> PrivateType` is an error because
    /// external code could call `foo` but cannot name its return type.
    PublicItemLeak {
        /// The public item that has the visibility leak
        item_name: String,
        /// The kind of item (function, type alias, etc.)
        item_kind: String,
        /// The required visibility level (e.g., "public")
        required_visibility: String,
        /// Chain showing how the private type is reached
        /// e.g., ["MyAlias", "InnerPrivate"] for a type alias chain
        leak_path: Vec<String>,
        /// The actual visibility of the leaked type (e.g., "private")
        leaked_visibility: String,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Type errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Type mismatch between expected and actual
    TypeMismatch { expected: Type, found: Type },

    /// Cannot infer type (need annotation)
    CannotInferType,

    /// Cannot infer type argument for a polymorphic function
    CannotInferTypeArg(String),

    /// Wrong number of arguments
    ArityMismatch { expected: usize, found: usize },

    /// Expected a function type
    ExpectedFunction(Type),

    /// Expected a specific type
    ExpectedType { expected: String, found: Type },

    // ─────────────────────────────────────────────────────────────────────────
    // Phase 1 restrictions
    // ─────────────────────────────────────────────────────────────────────────
    /// Feature not supported in Phase 1
    UnsupportedFeature(String),

    /// Return statements not supported
    ReturnNotSupported,

    /// Mutability not supported
    MutabilityNotSupported,

    // ─────────────────────────────────────────────────────────────────────────
    // Pattern matching errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Non-exhaustive pattern match
    NonExhaustiveMatch,

    /// Unreachable match arm (after catch-all)
    UnreachableArm,

    /// Pattern nesting too deep
    PatternTooDeep { depth: usize, max: usize },

    /// Pattern not supported
    UnsupportedPattern(String),

    // ─────────────────────────────────────────────────────────────────────────
    // Entry point errors (for compile/run)
    // ─────────────────────────────────────────────────────────────────────────
    /// No main function found
    NoMainFunction,

    /// File contains `sorry` (cannot compile)
    ContainsSorry,

    // ─────────────────────────────────────────────────────────────────────────
    // Other errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Generic error with custom message
    Other(String),
}

impl ElabError {
    /// Create a new elaboration error.
    pub fn new(span: Span, kind: ElabErrorKind) -> Self {
        let message = kind.default_message();
        Self {
            message,
            span,
            kind,
            notes: Vec::new(),
            help: None,
            context: None,
            file_path: None,
        }
    }

    /// Create an error with a custom message.
    pub fn with_message(span: Span, kind: ElabErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            kind,
            notes: Vec::new(),
            help: None,
            context: None,
            file_path: None,
        }
    }

    /// Add context explaining why a type was expected.
    #[must_use]
    pub fn with_context(mut self, context: ExpectedContext) -> Self {
        // Convert context to a span note for rendering
        self.notes.push(Note {
            message: context.explanation(),
            span: Some(context.span),
        });
        self.context = Some(context);
        self
    }

    /// Add a note to this error.
    #[must_use]
    pub fn with_note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span: None,
        });
        self
    }

    /// Add a note with a span to this error.
    #[must_use]
    pub fn with_span_note(mut self, span: Span, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            message: message.into(),
            span: Some(span),
        });
        self
    }

    /// Add help text to this error.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Set the file path where this error occurred.
    #[must_use]
    pub fn with_file_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.file_path = Some(path.into());
        self
    }

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

    // ─────────────────────────────────────────────────────────────────────────
    // Convenience constructors
    // ─────────────────────────────────────────────────────────────────────────

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

    /// Create a "return not supported" error.
    pub fn return_not_supported(span: Span) -> Self {
        Self::new(span, ElabErrorKind::ReturnNotSupported)
            .with_help("use a trailing expression instead of `return`")
    }

    /// Create a "duplicate definition" error.
    pub fn duplicate(span: Span, name: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::DuplicateDefinition(name.into()))
    }

    /// Create a "module not found" error.
    pub fn module_not_found(span: Span, module: impl Into<String>) -> Self {
        Self::new(
            span,
            ElabErrorKind::ModuleNotFound {
                module: module.into(),
                suggestion: None,
            },
        )
    }

    /// Create a "module not found" error with a suggestion.
    pub fn module_not_found_with_suggestion(
        span: Span,
        module: impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::ModuleNotFound {
                module: module.into(),
                suggestion,
            },
        )
    }

    /// Create an "item not found in module" error.
    pub fn item_not_in_module(
        span: Span,
        module: impl Into<String>,
        item: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::ItemNotFoundInModule {
                module: module.into(),
                item: item.into(),
            },
        )
    }

    /// Create a "duplicate import" error showing both import locations.
    ///
    /// The error will show:
    /// - Primary span at `second_import_span` (the import that triggered the error)
    /// - Secondary note at `first_import_span` showing where it was first imported
    /// - Different message format when imports come from different modules
    pub fn duplicate_import(
        second_import_span: Span,
        name: impl Into<String>,
        first_import_span: Span,
        first_source_module: impl Into<String>,
        second_source_module: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let first_source = first_source_module.into();
        let second_source = second_source_module.into();

        let mut err = Self::new(
            second_import_span,
            ElabErrorKind::DuplicateImport {
                name: name.clone(),
                first_import_span,
                second_import_span,
                first_source_module: first_source.clone(),
                second_source_module: second_source.clone(),
            },
        );

        // Add note showing first import location
        if first_source == second_source {
            err = err.with_span_note(first_import_span, "first imported here");
        } else {
            err = err.with_span_note(
                first_import_span,
                format!("first imported from `{}`", first_source),
            );
        }

        // Add help suggesting `as` rename
        err = err.with_help(format!(
            "use `as` to rename one import: `use {}::{} as {}_alias;`",
            second_source,
            name,
            name.to_lowercase()
        ));

        err
    }

    /// Create a "glob conflict" error for when two glob imports bring in the same name.
    pub fn glob_conflict(
        span: Span,
        name: impl Into<String>,
        first_module: impl Into<String>,
        second_module: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::GlobConflict {
                name: name.into(),
                first_module: first_module.into(),
                second_module: second_module.into(),
            },
        )
    }

    /// Create an "unresolved import" error.
    pub fn unresolved_import(span: Span, path: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UnresolvedImport(path.into()))
    }

    /// Create a "private module" error.
    pub fn private_module(
        span: Span,
        module_path: impl Into<String>,
        accessed_from: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::PrivateModule {
                module_path: module_path.into(),
                accessed_from: accessed_from.into(),
            },
        )
    }

    /// Create a "private item" error.
    pub fn private_item(
        span: Span,
        item_name: impl Into<String>,
        item_kind: impl Into<String>,
        defined_in: impl Into<String>,
        accessed_from: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::PrivateItem {
                item_name: item_name.into(),
                item_kind: item_kind.into(),
                defined_in: defined_in.into(),
                accessed_from: accessed_from.into(),
            },
        )
    }

    /// Create a "public item leak" error.
    ///
    /// This error is reported when a public (or crate-public) item's signature
    /// references a type with less visibility than the item itself.
    pub fn public_item_leak(
        span: Span,
        item_name: impl Into<String>,
        item_kind: impl Into<String>,
        required_visibility: impl Into<String>,
        leak_path: Vec<String>,
        leaked_visibility: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::PublicItemLeak {
                item_name: item_name.into(),
                item_kind: item_kind.into(),
                required_visibility: required_visibility.into(),
                leak_path,
                leaked_visibility: leaked_visibility.into(),
            },
        )
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
}

impl ElabErrorKind {
    /// Get the default message for this error kind.
    fn default_message(&self) -> String {
        match self {
            ElabErrorKind::UndefinedVariable(name) => {
                format!("cannot find value `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedType(name) => {
                format!("cannot find type `{}` in this scope", name)
            }
            ElabErrorKind::UndefinedConstructor(name) => {
                format!("cannot find constructor `{}` in this scope", name)
            }
            ElabErrorKind::DuplicateDefinition(name) => {
                format!("the name `{}` is defined multiple times", name)
            }
            ElabErrorKind::ModuleNotFound { module, suggestion } => match suggestion {
                Some(s) => format!("cannot find module `{}`; did you mean `{}`?", module, s),
                None => format!("cannot find module `{}`", module),
            },
            ElabErrorKind::ItemNotFoundInModule { module, item } => {
                format!("cannot find `{}` in module `{}`", item, module)
            }
            ElabErrorKind::DuplicateImport {
                name,
                first_source_module,
                second_source_module,
                ..
            } => {
                if first_source_module == second_source_module {
                    format!("the name `{}` is imported multiple times", name)
                } else {
                    format!(
                        "the name `{}` is imported from both `{}` and `{}`",
                        name, first_source_module, second_source_module
                    )
                }
            }
            ElabErrorKind::GlobConflict {
                name,
                first_module,
                second_module,
            } => {
                format!(
                    "`{}` is imported from both `{}::*` and `{}::*`",
                    name, first_module, second_module
                )
            }
            ElabErrorKind::UnresolvedImport(path) => {
                format!("cannot resolve import `{}`", path)
            }
            ElabErrorKind::PrivateModule {
                module_path,
                accessed_from,
            } => {
                format!(
                    "module `{}` is private and cannot be accessed from `{}`",
                    module_path, accessed_from
                )
            }
            ElabErrorKind::PrivateItem {
                item_name,
                item_kind,
                defined_in,
                accessed_from,
            } => {
                format!(
                    "{} `{}` is private (defined in `{}`) and cannot be accessed from `{}`",
                    item_kind, item_name, defined_in, accessed_from
                )
            }
            ElabErrorKind::PublicItemLeak {
                item_name,
                item_kind,
                required_visibility,
                leak_path,
                leaked_visibility,
            } => {
                let path_str = leak_path.join(" -> ");
                format!(
                    "{} {} `{}` exposes {} type `{}` in its signature (via: {})",
                    required_visibility,
                    item_kind,
                    item_name,
                    leaked_visibility,
                    leak_path.last().unwrap_or(&item_name.clone()),
                    path_str
                )
            }
            ElabErrorKind::TypeMismatch { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }
            ElabErrorKind::CannotInferType => {
                "cannot infer type; add a type annotation".to_string()
            }
            ElabErrorKind::CannotInferTypeArg(var) => {
                format!(
                    "cannot infer type argument `{}`; provide explicit type arguments",
                    var
                )
            }
            ElabErrorKind::ArityMismatch { expected, found } => {
                format!("expected {} arguments, found {}", expected, found)
            }
            ElabErrorKind::ExpectedFunction(ty) => {
                format!("expected function, found `{}`", ty)
            }
            ElabErrorKind::ExpectedType { expected, found } => {
                format!("expected `{}`, found `{}`", expected, found)
            }
            ElabErrorKind::UnsupportedFeature(feature) => {
                format!("`{}` is not supported", feature)
            }
            ElabErrorKind::ReturnNotSupported => {
                "`return` is not supported; use trailing expression".to_string()
            }
            ElabErrorKind::MutabilityNotSupported => {
                "mutable bindings are not supported; use shadowing instead".to_string()
            }
            ElabErrorKind::NonExhaustiveMatch => "non-exhaustive match patterns".to_string(),
            ElabErrorKind::UnreachableArm => "unreachable pattern".to_string(),
            ElabErrorKind::PatternTooDeep { depth, max } => {
                format!("pattern nesting depth {} exceeds maximum of {}", depth, max)
            }
            ElabErrorKind::UnsupportedPattern(pat) => {
                format!("pattern `{}` is not supported", pat)
            }
            ElabErrorKind::NoMainFunction => "no `main` function found".to_string(),
            ElabErrorKind::ContainsSorry => "cannot compile file containing `sorry`".to_string(),
            ElabErrorKind::Other(msg) => msg.clone(),
        }
    }

    /// Get the error code for this kind.
    pub fn code(&self) -> &'static str {
        match self {
            ElabErrorKind::UndefinedVariable(_) => "E0001",
            ElabErrorKind::UndefinedType(_) => "E0002",
            ElabErrorKind::UndefinedConstructor(_) => "E0003",
            ElabErrorKind::DuplicateDefinition(_) => "E0004",
            ElabErrorKind::ModuleNotFound { .. } => "E0005",
            ElabErrorKind::ItemNotFoundInModule { .. } => "E0006",
            ElabErrorKind::DuplicateImport { .. } => "E0007",
            ElabErrorKind::GlobConflict { .. } => "E0018",
            ElabErrorKind::UnresolvedImport(_) => "E0008",
            ElabErrorKind::PrivateModule { .. } => "E0009",
            ElabErrorKind::PrivateItem { .. } => "E0016",
            ElabErrorKind::PublicItemLeak { .. } => "E0017",
            ElabErrorKind::TypeMismatch { .. } => "E0010",
            ElabErrorKind::CannotInferType => "E0011",
            ElabErrorKind::CannotInferTypeArg(_) => "E0015",
            ElabErrorKind::ArityMismatch { .. } => "E0012",
            ElabErrorKind::ExpectedFunction(_) => "E0013",
            ElabErrorKind::ExpectedType { .. } => "E0014",
            ElabErrorKind::UnsupportedFeature(_) => "E0100",
            ElabErrorKind::ReturnNotSupported => "E0101",
            ElabErrorKind::MutabilityNotSupported => "E0102",
            ElabErrorKind::NonExhaustiveMatch => "E0020",
            ElabErrorKind::UnreachableArm => "W0001",
            ElabErrorKind::PatternTooDeep { .. } => "E0103",
            ElabErrorKind::UnsupportedPattern(_) => "E0021",
            ElabErrorKind::NoMainFunction => "E0030",
            ElabErrorKind::ContainsSorry => "E0031",
            ElabErrorKind::Other(_) => "E9999",
        }
    }
}

impl fmt::Display for ElabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error[{}]: {} (at {}..{})",
            self.kind.code(),
            self.message,
            self.span.start,
            self.span.end
        )?;

        for note in &self.notes {
            if let Some(span) = note.span {
                write!(
                    f,
                    "\n  note: {} (at {}..{})",
                    note.message, span.start, span.end
                )?;
            } else {
                write!(f, "\n  note: {}", note.message)?;
            }
        }

        if let Some(ref help) = self.help {
            write!(f, "\n  help: {}", help)?;
        }

        Ok(())
    }
}

impl std::error::Error for ElabError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undefined_variable() {
        let err = ElabError::undefined_variable(Span::new(10, 13), "foo");
        assert!(err.message.contains("foo"));
        assert!(err.message.contains("cannot find"));
        assert_eq!(err.kind.code(), "E0001");
    }

    #[test]
    fn test_type_mismatch() {
        let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat);
        assert!(err.message.contains("Bool"));
        assert!(err.message.contains("Nat"));
        assert_eq!(err.kind.code(), "E0010");
    }

    #[test]
    fn test_error_with_notes() {
        let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat)
            .with_note("expected due to return type")
            .with_help("try converting with `to_bool()`");

        assert_eq!(err.notes.len(), 1);
        assert!(err.help.is_some());
    }

    #[test]
    fn test_display() {
        let err = ElabError::undefined_variable(Span::new(10, 13), "foo")
            .with_help("did you mean `for`?");

        let s = format!("{}", err);
        assert!(s.contains("E0001"));
        assert!(s.contains("foo"));
        assert!(s.contains("did you mean"));
    }
}
