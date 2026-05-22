//! Duplicate import error tests (ADR 28.1.26: E0007 improvements).

use crate::elaborate::error::{DuplicateImportInfo, ElabError, ElabErrorKind};
use crate::span::Span;

#[test]
fn test_error_duplicate_import_structure() {
    // Test that DuplicateImport error stores both import spans and module info
    let first_span = Span::new(10, 20);
    let second_span = Span::new(50, 60);

    let error = ElabError::duplicate_import(
        second_span,
        DuplicateImportInfo {
            name: "Span".to_string(),
            source_name: "Span".to_string(),
            first_import_span: first_span,
            first_source_module: "lexer::span".to_string(),
            second_source_module: "parser::ast".to_string(),
        },
    );

    // Verify error kind
    assert!(matches!(
        error.kind,
        ElabErrorKind::DuplicateImport {
            name,
            first_import_span,
            second_import_span,
            first_source_module,
            second_source_module,
        } if name == "Span"
            && first_import_span == first_span
            && second_import_span == second_span
            && first_source_module == "lexer::span"
            && second_source_module == "parser::ast"
    ));

    // Primary span should be second import (the one that triggered the error)
    assert_eq!(error.span, second_span);

    // Should have note about first import
    assert!(error
        .notes
        .iter()
        .any(|n| n.message.contains("first imported") && n.span == Some(first_span)));

    // Should have help suggesting `as` rename
    assert!(error.help.as_ref().map_or(false, |h| h.contains("as")));
}

#[test]
fn test_error_duplicate_import_same_module_message() {
    // When both imports come from same module, message should be simpler
    let error = ElabError::duplicate_import(
        Span::new(50, 60),
        DuplicateImportInfo {
            name: "Span".to_string(),
            source_name: "Span".to_string(),
            first_import_span: Span::new(10, 20),
            first_source_module: "lexer::span".to_string(),
            second_source_module: "lexer::span".to_string(), // same module
        },
    );

    // Note should say "first imported here" (not "first imported from `X`")
    assert!(error
        .notes
        .iter()
        .any(|n| n.message == "first imported here"));
}

#[test]
fn test_error_duplicate_import_different_modules_message() {
    // When imports come from different modules, message should mention source
    let error = ElabError::duplicate_import(
        Span::new(50, 60),
        DuplicateImportInfo {
            name: "Span".to_string(),
            source_name: "Span".to_string(),
            first_import_span: Span::new(10, 20),
            first_source_module: "lexer::span".to_string(),
            second_source_module: "parser::ast".to_string(), // different module
        },
    );

    // Note should mention the first source module
    assert!(error
        .notes
        .iter()
        .any(|n| n.message.contains("lexer::span")));
}

#[test]
fn test_error_duplicate_import_alias_help_uses_source_name() {
    // When `use b::Bar as Foo` clashes with `use a::Foo`, the help text
    // should suggest `use parser::ast::Token as token_alias;` (source name),
    // not `use parser::ast::Span as span_alias;` (local alias name).
    let error = ElabError::duplicate_import(
        Span::new(50, 60),
        DuplicateImportInfo {
            name: "Span".to_string(),         // local alias that clashed
            source_name: "Token".to_string(), // actual source name in the second module
            first_import_span: Span::new(10, 20),
            first_source_module: "lexer::span".to_string(),
            second_source_module: "parser::ast".to_string(),
        },
    );

    let help = error.help.as_ref().expect("should have help text");
    assert!(
        help.contains("parser::ast::Token"),
        "help should reference source name 'Token', got: {help}"
    );
    assert!(
        help.contains("token_alias"),
        "help should suggest 'token_alias', got: {help}"
    );
    assert!(
        !help.contains("parser::ast::Span"),
        "help should NOT use local alias 'Span' in the path, got: {help}"
    );
}
