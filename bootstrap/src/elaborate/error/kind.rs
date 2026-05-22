//! Categories of elaboration errors with default messages and error codes.
//!
//! **L1/L2 error code numbering differs.** L1 (this file) uses a flat numbering
//! scheme (E0001–E9999). L2 (`src/compiler/elab/error/kinds.tg`) uses range-based
//! numbering (E0001–E0099 for types, E0100–E0199 for names, etc.).
//!
//! Key divergences:
//!   L1 UndefinedVariable  = E0001   vs  L2 ErrUnresolvedValue  = E0101
//!   L1 TypeMismatch       = E0010   vs  L2 ErrTypeMismatch     = E0001
//!   L1 UndefinedType      = E0002   vs  L2 ErrUnresolvedType   = E0100
//!
//! `.tg` test assertions (expect_error, expect_type) use L1 codes since L1 runs them.

use serde::{Deserialize, Serialize};

use crate::span::Span;
use tungsten_core::{Term, Type};

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

    /// Mutability not supported
    MutabilityNotSupported,

    // ─────────────────────────────────────────────────────────────────────────
    // Pattern matching errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Non-exhaustive pattern match
    NonExhaustiveMatch,

    /// Unreachable match arm (after catch-all)
    UnreachableArm,

    /// Dead code after `return` expression (ADR 13.5.26d)
    DeadCodeAfterReturn,

    /// Pattern nesting too deep
    PatternTooDeep { depth: usize, max: usize },

    /// Pattern not supported
    UnsupportedPattern(String),

    // ─────────────────────────────────────────────────────────────────────────
    // Try operator errors (ADR 13.5.26e)
    // ─────────────────────────────────────────────────────────────────────────
    /// `?` used on a type that is not `Result` or `Option`
    TryOnNonTryType(String),

    /// `?` return type mismatch
    TryReturnMismatch {
        operand_type: String,
        return_type: String,
    },

    /// `?` used outside a function/closure body
    TryOutsideReturnContext,

    /// explicit `return` inside a `try` block (ADR 15.5.26d)
    ReturnInsideTryBlock,

    /// `try` block requires a `Result` type annotation (ADR 15.5.26d)
    TryBlockRequiresResultType,

    /// `try` block expected Sum encoding but found something else (ADR 15.5.26d)
    TryBlockExpectedSumEncoding,

    /// `try` block Result type missing Ok or Err constructor (ADR 15.5.26d)
    TryBlockMissingConstructor(String),

    // ─────────────────────────────────────────────────────────────────────────
    // Let-else errors (ADR 13.5.26f)
    // ─────────────────────────────────────────────────────────────────────────
    /// `else` branch in `let`-`else` does not diverge.
    /// NOTE: Currently unreachable — match desugaring catches non-diverging
    /// else branches as E0010 (TypeMismatch). Kept as a reserved code for
    /// potential future dedicated diagnostic.
    LetElseNonDiverging(String),

    /// Irrefutable pattern in `let`-`else` (warning)
    LetElseIrrefutable,

    /// Irrefutable pattern in `if let` (warning)
    IfLetIrrefutable,

    // ─────────────────────────────────────────────────────────────────────────
    // Named record errors (ADR 13.5.26h)
    // ─────────────────────────────────────────────────────────────────────────
    /// Type used in named record constructor is not a record type
    NotARecordType(String),

    /// Missing field in named record constructor
    MissingRecordField { field: String, type_name: String },

    /// Extra (unknown) field in named record constructor
    ExtraRecordField { field: String, type_name: String },

    /// Duplicate field in named record constructor
    DuplicateRecordField(String),

    // ─────────────────────────────────────────────────────────────────────────
    // Entry point errors (for compile/run)
    // ─────────────────────────────────────────────────────────────────────────
    /// No main function found
    NoMainFunction,

    /// File contains `sorry` (cannot compile)
    ContainsSorry,

    // ─────────────────────────────────────────────────────────────────────────
    // Type alias errors (ADR 15.5.26g)
    // ─────────────────────────────────────────────────────────────────────────
    /// Recursive type alias cycle
    RecursiveAlias(String),

    // ─────────────────────────────────────────────────────────────────────────
    // Equality proof errors (ADR 21.5.26d)
    // ─────────────────────────────────────────────────────────────────────────
    /// `refl` checked against a non-equality type
    ReflExpectedEquality(Type),

    /// `refl` checked against an equality type whose sides are not definitionally equal
    InvalidRefl { left: Term, right: Term },

    /// `subst` proof argument does not have an equality type
    SubstExpectedEquality(Type),

    /// `trans` endpoints don't match: h1's right side ≠ h2's left side
    TransEndpointMismatch { left: Term, right: Term },

    /// `cong` first argument is not a function type
    CongExpectedFunction(Type),

    // ─────────────────────────────────────────────────────────────────────────
    // Motive errors (ADR 21.5.26g)
    // ─────────────────────────────────────────────────────────────────────────
    /// `subst` motive is not a predicate lambda (e.g., a literal or non-lambda expression)
    MotiveNotPredicate(Type),

    /// `subst` motive binder type does not match equality base type
    MotiveDomainMismatch { expected: Type, found: Type },

    /// `subst` motive body is not a valid type expression
    MotiveBodyNotType,

    /// `natind` motive domain is not `Nat` (ADR 22.5.26a)
    NatIndMotiveNotNat(Type),

    // ─────────────────────────────────────────────────────────────────────────
    // Other errors
    // ─────────────────────────────────────────────────────────────────────────
    /// Generic error with custom message
    Other(String),
}

/// Generates `ElabErrorKind::code()` from a flat variant→code table.
macro_rules! error_codes {
    ($($pat:pat => $code:expr),* $(,)?) => {
        impl ElabErrorKind {
            /// Get the error code for this kind.
            pub fn code(&self) -> &'static str {
                match self {
                    $($pat => $code,)*
                }
            }
        }
    };
}

error_codes! {
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
    ElabErrorKind::MutabilityNotSupported => "E0102",
    ElabErrorKind::NonExhaustiveMatch => "E0020",
    ElabErrorKind::UnreachableArm => "W0001",
    ElabErrorKind::DeadCodeAfterReturn => "W0002",
    ElabErrorKind::PatternTooDeep { .. } => "E0103",
    ElabErrorKind::UnsupportedPattern(_) => "E0021",
    ElabErrorKind::TryOnNonTryType(_) => "E0040",
    ElabErrorKind::TryReturnMismatch { .. } => "E0041",
    ElabErrorKind::TryOutsideReturnContext => "E0042",
    ElabErrorKind::ReturnInsideTryBlock => "E0044",
    ElabErrorKind::TryBlockRequiresResultType => "E0045",
    ElabErrorKind::TryBlockExpectedSumEncoding => "E0046",
    ElabErrorKind::TryBlockMissingConstructor(_) => "E0047",
    ElabErrorKind::LetElseNonDiverging(_) => "E0043",
    ElabErrorKind::LetElseIrrefutable => "W0003",
    ElabErrorKind::IfLetIrrefutable => "W0004",
    ElabErrorKind::NotARecordType(_) => "E0050",
    ElabErrorKind::MissingRecordField { .. } => "E0051",
    ElabErrorKind::ExtraRecordField { .. } => "E0052",
    ElabErrorKind::DuplicateRecordField(_) => "E0053",
    ElabErrorKind::NoMainFunction => "E0030",
    ElabErrorKind::ContainsSorry => "E0031",
    ElabErrorKind::RecursiveAlias(_) => "E0060",
    ElabErrorKind::ReflExpectedEquality(_) => "E0070",
    ElabErrorKind::InvalidRefl { .. } => "E0071",
    ElabErrorKind::SubstExpectedEquality(_) => "E0072",
    ElabErrorKind::TransEndpointMismatch { .. } => "E0073",
    ElabErrorKind::CongExpectedFunction(_) => "E0074",
    ElabErrorKind::MotiveNotPredicate(_) => "E0075",
    ElabErrorKind::MotiveDomainMismatch { .. } => "E0076",
    ElabErrorKind::MotiveBodyNotType => "E0077",
    ElabErrorKind::NatIndMotiveNotNat(_) => "E0078",
    ElabErrorKind::Other(_) => "E9999",
}
