//! Tests for the `tungsten explain` module.

use super::error_catalogue;
use super::type_parser::{self, TypeAst};
use super::{cmd_explain, ExplainCommands};
use std::process::ExitCode;

// ─────────────────────────────────────────────────────────────────────────────
// Error catalogue tests
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that every known error kind name has a corresponding explanation.
#[test]
fn all_error_kinds_have_explanations() {
    let names = error_catalogue::all_known_names();
    assert!(
        !names.is_empty(),
        "CATEGORIES should contain at least one error kind"
    );
    for name in &names {
        assert!(
            error_catalogue::get_explanation_by_name(name),
            "missing explanation for error kind: {}",
            name
        );
    }
}

/// Verify the catalogue covers all ElabErrorKind variants.
///
/// This list must be kept in sync with `ElabErrorKind` in
/// `bootstrap/src/elaborate/error/kind.rs`. When a new variant is added
/// there, add it here too — the test will fail if they diverge.
#[test]
fn catalogue_covers_all_elab_error_kinds() {
    let expected_variants = [
        // Name Resolution
        "UndefinedVariable",
        "UndefinedType",
        "UndefinedConstructor",
        "DuplicateDefinition",
        "ModuleNotFound",
        "ItemNotFoundInModule",
        "DuplicateImport",
        "GlobConflict",
        "UnresolvedImport",
        "PrivateModule",
        "PrivateItem",
        "PublicItemLeak",
        // Type Errors
        "TypeMismatch",
        "CannotInferType",
        "CannotInferTypeArg",
        "ArityMismatch",
        "ExpectedFunction",
        "ExpectedType",
        // Phase 1 Restrictions
        "UnsupportedFeature",
        "MutabilityNotSupported",
        // Pattern Matching
        "NonExhaustiveMatch",
        "UnreachableArm",
        "DeadCodeAfterReturn",
        "PatternTooDeep",
        "UnsupportedPattern",
        // Control Flow
        "DeadCodeAfterReturn",
        "TryOnNonTryType",
        "TryReturnMismatch",
        "TryOutsideReturnContext",
        "LetElseNonDiverging",
        "LetElseIrrefutable",
        // Named Records
        "NotARecordType",
        "MissingRecordField",
        "ExtraRecordField",
        "DuplicateRecordField",
        // Entry Point
        "NoMainFunction",
        "ContainsSorry",
        // Note: Other(String) is intentionally excluded — it's a catch-all
    ];

    let known = error_catalogue::all_known_names();
    for variant in &expected_variants {
        assert!(
            known.contains(variant),
            "ElabErrorKind variant `{}` is not in the error catalogue CATEGORIES",
            variant
        );
        assert!(
            error_catalogue::get_explanation_by_name(variant),
            "ElabErrorKind variant `{}` has no explanation in get_explanation()",
            variant
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type parser tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn parse_simple_base_type() {
    assert_eq!(
        type_parser::parse_type("Nat").unwrap(),
        TypeAst::Base("Nat".into())
    );
}

#[test]
fn parse_arrow_type() {
    let ast = type_parser::parse_type("(Nat → Bool)").unwrap();
    assert_eq!(
        ast,
        TypeAst::Arrow(
            Box::new(TypeAst::Base("Nat".into())),
            Box::new(TypeAst::Base("Bool".into()))
        )
    );
}

#[test]
fn parse_recursive_list_type() {
    let ast = type_parser::parse_type("μα_List. (Unit + (Nat × α_List))").unwrap();
    let TypeAst::Mu(var, body) = &ast else {
        panic!("expected Mu, got {ast:?}");
    };
    assert_eq!(var, "α_List");
    let TypeAst::Sum(lhs, rhs) = body.as_ref() else {
        panic!("expected Sum, got {body:?}");
    };
    assert_eq!(**lhs, TypeAst::Base("Unit".into()));
    let TypeAst::Product(f1, f2) = rhs.as_ref() else {
        panic!("expected Product, got {rhs:?}");
    };
    assert_eq!(**f1, TypeAst::Base("Nat".into()));
    assert_eq!(**f2, TypeAst::TyVar("α_List".into()));
}

#[test]
fn parse_forall_identity() {
    let ast = type_parser::parse_type("∀T. (T → T)").unwrap();
    assert_eq!(
        ast,
        TypeAst::Forall(
            "T".into(),
            Box::new(TypeAst::Arrow(
                Box::new(TypeAst::TyVar("T".into())),
                Box::new(TypeAst::TyVar("T".into()))
            ))
        )
    );
}

#[test]
fn parse_named_type_variable() {
    let ast = type_parser::parse_type("@Point").unwrap();
    assert_eq!(ast, TypeAst::TyVar("@Point".into()));
}

#[test]
fn parse_error_on_empty() {
    assert!(type_parser::parse_type("").is_err());
}

#[test]
fn parse_error_on_malformed() {
    assert!(type_parser::parse_type("(Nat →").is_err());
    assert!(type_parser::parse_type("μ").is_err());
}

#[test]
fn parse_type_error_placeholder() {
    assert_eq!(
        type_parser::parse_type("<type error>").unwrap(),
        TypeAst::Error
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests — exit codes for each explain subcommand
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn exit_code_error_list() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: None,
        l2: false,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_error_known_kind() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("TypeMismatch".into()),
        l2: false,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_error_unknown_kind() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("BogusErrorName".into()),
        l2: false,
    });
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn exit_code_l2_error_list() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: None,
        l2: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_l2_error_known_code() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("E0001".into()),
        l2: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_l2_error_known_name() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("ErrTypeMismatch".into()),
        l2: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_l2_error_unknown() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("E9999".into()),
        l2: true,
    });
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn exit_code_type_simple() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "Nat".into(),
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_type_arrow() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "Nat → Bool".into(),
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_type_recursive() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "μα_List. (Unit + (Nat × α_List))".into(),
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_type_forall() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "∀T. (T → T)".into(),
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_type_malformed() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "(broken →".into(),
    });
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn exit_code_type_empty() {
    let code = cmd_explain(ExplainCommands::Type {
        type_string: "".into(),
    });
    assert_eq!(code, ExitCode::FAILURE);
}
