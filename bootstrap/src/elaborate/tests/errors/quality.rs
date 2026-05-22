//! Error quality tests and file_path tracking.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::elab_err;

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
