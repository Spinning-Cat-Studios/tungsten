//! Tests for error cases in elaboration.

use super::elab_err;
use crate::elaborate::error::ElabErrorKind;

// ─────────────────────────────────────────────────────────────────────────────
// Error cases
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_undefined_variable() {
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            undefined_var
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::UndefinedVariable(_)
    ));
}

#[test]
fn test_error_undefined_variable_with_suggestion() {
    // When there's a similar variable name, suggest it
    let errors = elab_err(
        r#"
        fn test(value: Nat) -> Nat {
            valu
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::UndefinedVariable(_)
    ));
    // Check that help contains a suggestion
    assert!(errors[0]
        .help
        .as_ref()
        .map_or(false, |h| h.contains("value")));
}

#[test]
fn test_error_undefined_type() {
    let errors = elab_err(
        r#"
        fn test(x: UndefinedType) -> Nat {
            0
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::UndefinedType(_)));
}

#[test]
fn test_error_undefined_type_with_suggestion() {
    // When there's a similar type name, suggest it
    let errors = elab_err(
        r#"
        fn test() -> Boo {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::UndefinedType(_)));
    // Check that help contains a suggestion
    assert!(errors[0]
        .help
        .as_ref()
        .map_or(false, |h| h.contains("Bool")));
}

#[test]
fn test_error_undefined_constructor_with_suggestion() {
    // When there's a similar constructor name in a pattern, suggest it
    // Use Gren() with parens to ensure it's parsed as a constructor pattern
    let errors = elab_err(
        r#"
        enum Color { Red, Green, Blue }
        fn test(c: Color) -> Nat {
            match c {
                Gren() => 1,
                _ => 0
            }
        }
    "#,
    );
    assert!(!errors.is_empty(), "should have errors");
    assert!(
        matches!(errors[0].kind, ElabErrorKind::UndefinedConstructor(_)),
        "expected UndefinedConstructor, got: {:?}",
        errors[0].kind
    );
    // Check that help contains a suggestion
    let help = errors[0]
        .help
        .as_ref()
        .expect("should have help suggestion");
    assert!(
        help.contains("Green"),
        "should suggest Green for Gren, got: {}",
        help
    );
}

#[test]
fn test_error_type_mismatch() {
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
}

#[test]
fn test_error_type_mismatch_with_context() {
    // Type mismatch should include context about why the type was expected
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
    // Check that context is set (appears as a note with span)
    assert!(errors[0].notes.iter().any(|n| n.span.is_some()));
}

#[test]
fn test_error_return_not_supported() {
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            return 42
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::ReturnNotSupported));
}

#[test]
fn test_error_duplicate_definition() {
    let errors = elab_err(
        r#"
        fn foo() -> Nat { 0 }
        fn foo() -> Nat { 1 }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::DuplicateDefinition(_)
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// Error quality tests (Step 4)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_function_arg_context() {
    // Function argument errors should show context about which argument
    let errors = elab_err(
        r#"
        fn add(x: Nat, y: Nat) -> Nat { x + y }
        fn test() -> Nat {
            add(1, true)
        }
    "#,
    );
    assert!(!errors.is_empty(), "should have type mismatch error");
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
    // Should have context note about argument position
    let has_arg_context = errors[0]
        .notes
        .iter()
        .any(|n| n.message.contains("argument") && n.span.is_some());
    assert!(has_arg_context, "should have context about which argument");
}

#[test]
fn test_error_if_branch_context() {
    // If branch mismatch should point to the other branch
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            let x = if true { 42 } else { false };
            x
        }
    "#,
    );
    assert!(!errors.is_empty(), "should have type mismatch error");
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
    // Should have context about branch unification
    let has_branch_context = errors[0]
        .notes
        .iter()
        .any(|n| n.message.contains("branch") && n.span.is_some());
    assert!(
        has_branch_context,
        "should have context about branch unification"
    );
}
// ─────────────────────────────────────────────────────────────────────────────
// Error file_path tracking tests (ADR 4.1: Better Error Messages)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_has_file_path_when_set_via_record_error() {
    // Test that errors going through record_error() get file_path attached
    // when the elaborator has module info set up.
    // Note: In single-file elaboration without module info, file_path will be None.
    // This test verifies the error structure itself supports file_path.
    use crate::elaborate::error::ElabError;
    use crate::span::Span;
    use std::path::PathBuf;

    let span = Span::new(0, 10);
    let error = ElabError::type_mismatch(span, tungsten_core::Type::Nat, tungsten_core::Type::Bool);

    // Without file_path
    assert!(error.file_path.is_none());

    // With file_path attached
    let error_with_path = error.with_file_path(PathBuf::from("test/foo.tg"));
    assert!(error_with_path.file_path.is_some());
    assert_eq!(
        error_with_path
            .file_path
            .as_ref()
            .unwrap()
            .to_str()
            .unwrap(),
        "test/foo.tg"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Duplicate Import Error Tests (ADR 28.1.26: E0007 improvements)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_duplicate_import_structure() {
    // Test that DuplicateImport error stores both import spans and module info
    use crate::elaborate::error::ElabError;
    use crate::span::Span;

    let first_span = Span::new(10, 20);
    let second_span = Span::new(50, 60);

    let error = ElabError::duplicate_import(
        second_span,
        "Span",
        first_span,
        "lexer::span",
        "parser::ast",
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
    use crate::elaborate::error::ElabError;
    use crate::span::Span;

    let error = ElabError::duplicate_import(
        Span::new(50, 60),
        "Span",
        Span::new(10, 20),
        "lexer::span",
        "lexer::span", // same module
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
    use crate::elaborate::error::ElabError;
    use crate::span::Span;

    let error = ElabError::duplicate_import(
        Span::new(50, 60),
        "Span",
        Span::new(10, 20),
        "lexer::span",
        "parser::ast", // different module
    );

    // Note should mention the first source module
    assert!(error
        .notes
        .iter()
        .any(|n| n.message.contains("lexer::span")));
}
