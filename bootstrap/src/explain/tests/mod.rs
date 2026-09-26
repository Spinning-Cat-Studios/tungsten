//! Tests for the `tungsten explain` module.
//!
//! - [`catalogue`] — coverage of the error catalogue and the code lookup
//!   (ADR 8.8.26a): every code a diagnostic can print must resolve.
//! - [`help_claims`] — the catalogue's prose side (ADR 19.8.26b): what its
//!   descriptions CLAIM about the listing, against what the listing withholds.
//! - The type parser and exit-code tests stay here; they are about the
//!   `explain type` arm and the CLI shell, not the catalogue.

mod catalogue;
mod help_claims;

use super::type_parser::{self, TypeAst};
use super::{cmd_explain, ExplainCommands};
use std::process::ExitCode;

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
        self_hosted: false,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_error_known_kind() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("TypeMismatch".into()),
        self_hosted: false,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_error_unknown_kind() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("BogusErrorName".into()),
        self_hosted: false,
    });
    assert_eq!(code, ExitCode::FAILURE);
}

/// The unknown-**code** failure path, which is a different arm from the
/// unknown-**name** one above and prints different advice.
#[test]
fn exit_code_error_unknown_code() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("E0099".into()),
        self_hosted: false,
    });
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn exit_code_self_hosted_error_list() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: None,
        self_hosted: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_self_hosted_error_known_code() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("E0001".into()),
        self_hosted: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_self_hosted_error_known_name() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("ErrTypeMismatch".into()),
        self_hosted: true,
    });
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn exit_code_self_hosted_error_unknown() {
    let code = cmd_explain(ExplainCommands::Error {
        kind: Some("E9999".into()),
        self_hosted: true,
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
