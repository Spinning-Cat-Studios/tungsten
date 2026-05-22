//! Tests for Phase A.5 failure guardrails (ADR 13.5.26g).

use crate::elaborate::{ElabError, ElabErrorKind};
use crate::span::Span;
use tungsten_core::Type;

/// Helper: create an E0001 (UndefinedVariable) error.
fn undefined_var_error(name: &str) -> ElabError {
    ElabError::new(
        Span::new(0, 0),
        ElabErrorKind::UndefinedVariable(name.to_string()),
    )
}

/// Helper: create an E0005 (ModuleNotFound) error.
fn module_not_found_error(module: &str) -> ElabError {
    ElabError::new(
        Span::new(0, 0),
        ElabErrorKind::ModuleNotFound {
            module: module.to_string(),
            suggestion: None,
        },
    )
}

/// Helper: create an E0010 (TypeMismatch) error — NOT a resolution error.
fn type_mismatch_error() -> ElabError {
    ElabError::new(
        Span::new(0, 0),
        ElabErrorKind::TypeMismatch {
            expected: Type::TyVar("Nat".to_string()),
            found: Type::TyVar("String".to_string()),
        },
    )
}

#[test]
fn annotate_adds_note_to_undefined_variable() {
    let mut errors = vec![undefined_var_error("foo")];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    assert_eq!(errors[0].notes.len(), 1);
    assert!(errors[0].notes[0].message.contains("Phase A.5"));
}

#[test]
fn annotate_adds_note_to_module_not_found() {
    let mut errors = vec![module_not_found_error("elab::env::resolve")];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    assert_eq!(errors[0].notes.len(), 1);
    assert!(errors[0].notes[0].message.contains("Phase A.5"));
}

#[test]
fn annotate_skips_non_resolution_errors() {
    let mut errors = vec![type_mismatch_error()];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    assert!(errors[0].notes.is_empty());
}

#[test]
fn annotate_mixed_errors_only_annotates_resolution() {
    let mut errors = vec![
        undefined_var_error("foo"),
        type_mismatch_error(),
        module_not_found_error("bad::path"),
    ];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    // E0001: annotated
    assert_eq!(errors[0].notes.len(), 1);
    // E0010: not annotated
    assert!(errors[1].notes.is_empty());
    // E0005: annotated
    assert_eq!(errors[2].notes.len(), 1);
}

#[test]
fn phase_a5_ok_defaults_to_true() {
    let acc = super::super::accumulator::ModuleTreeAccumulator::new();
    assert!(acc.phase_a5_ok);
}

#[test]
fn annotated_error_display_includes_hint() {
    let mut errors = vec![undefined_var_error("missing_fn")];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    let rendered = format!("{}", errors[0]);
    assert!(
        rendered.contains("note: Phase A.5 global collection failed"),
        "rendered error should contain Phase A.5 hint, got: {rendered}"
    );
    assert!(
        rendered.contains("tungsten doctor check phase-a5"),
        "rendered error should contain remediation command, got: {rendered}"
    );
}

#[test]
fn annotated_module_not_found_display_includes_hint() {
    let mut errors = vec![module_not_found_error("elab::env::resolve")];
    super::super::annotate_errors_for_phase_a5_failure(&mut errors);
    let rendered = format!("{}", errors[0]);
    assert!(
        rendered.contains("note:"),
        "rendered error should have a note line, got: {rendered}"
    );
    assert!(
        rendered.contains("bad import in another module"),
        "hint should mention bad import as root cause, got: {rendered}"
    );
}
