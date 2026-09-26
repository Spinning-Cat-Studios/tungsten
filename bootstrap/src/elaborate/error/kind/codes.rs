//! `ElabErrorKind::code()` — the flat variant→code table.
//!
//! Split from `kind/mod.rs` by ADR 11.8.26c, which added E0064 and took that
//! file over its size threshold. `error/` was already at the directory cap, so
//! `kind.rs` became `kind/` rather than gaining a sibling.
//!
//! **This file is an oracle, not just data.** `explain`'s catalogue
//! completeness test (`explain/tests/catalogue.rs`) parses the arms below
//! rather than a variant list typed beside it — the hand-maintained list it
//! replaced could not detect a variant missing from itself, and 15 were
//! (ADR 8.8.26a). So moving this table means updating that test's path, and
//! reformatting an arm across two lines would make it invisible to the parse.
//! One arm, one line.

use super::ElabErrorKind;

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
    ElabErrorKind::MatchScrutineeNotAdt { .. } => "E0022",
    ElabErrorKind::UnreachableArm => "W0001",
    ElabErrorKind::DeadCodeAfterReturn => "W0002",
    ElabErrorKind::PatternTooDeep { .. } => "E0103",
    ElabErrorKind::UnsupportedPattern(_) => "E0021",
    ElabErrorKind::TryOnNonTryType(_) => "E0040",
    ElabErrorKind::TryReturnMismatch { .. } => "E0041",
    ElabErrorKind::TryOutsideReturnContext => "E0042",
    ElabErrorKind::ReturnInsideTryBlock => "E0044",
    ElabErrorKind::ReturnOutsideFunction => "E0048",
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
    ElabErrorKind::NonStrictlyPositive { .. } => "E0061",
    ElabErrorKind::CannotProveTermination { .. } => "E0062",
    ElabErrorKind::PartialInProof { .. } => "E0063",
    ElabErrorKind::NestedRecursiveFamily { .. } => "E0064",
    ElabErrorKind::ReflExpectedEquality(_) => "E0070",
    ElabErrorKind::InvalidRefl { .. } => "E0071",
    ElabErrorKind::SubstExpectedEquality(_) => "E0072",
    ElabErrorKind::TransEndpointMismatch { .. } => "E0073",
    ElabErrorKind::CongExpectedFunction(_) => "E0074",
    ElabErrorKind::MotiveNotPredicate(_) => "E0075",
    ElabErrorKind::MotiveDomainMismatch { .. } => "E0076",
    ElabErrorKind::MotiveBodyNotType => "E0077",
    ElabErrorKind::NatIndMotiveNotNat(_) => "E0078",
    ElabErrorKind::ComparatorUnavailable(_) => "E0080",
    ElabErrorKind::IntLiteralOutOfRange(_) => "E0090",
    ElabErrorKind::BuiltinTypeRedefined(_) => "E0091",
    ElabErrorKind::InternalError(_) => "E9998",
    ElabErrorKind::Other(_) => "E9999",
}
