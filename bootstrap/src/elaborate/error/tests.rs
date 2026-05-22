//! Tests for ElabError, Note, TraceFrame, and ExpectedContext.

use super::*;
use std::path::Path;
use std::path::PathBuf;

use crate::span::Span;
use tungsten_core::Type;

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
    let err =
        ElabError::undefined_variable(Span::new(10, 13), "foo").with_help("did you mean `for`?");

    let s = format!("{}", err);
    assert!(s.contains("E0001"));
    assert!(s.contains("foo"));
    assert!(s.contains("did you mean"));
}

// ── ADR 15.5.26a: Multi-file diagnostic spans ──

#[test]
fn note_file_path_defaults_to_none() {
    // AC1: existing notes with file_path: None render identically (backward compat)
    let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat)
        .with_note("plain note")
        .with_span_note(Span::new(10, 15), "span note");

    for note in &err.notes {
        assert!(note.file_path.is_none());
    }
}

#[test]
fn cross_file_note_carries_path() {
    let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat)
        .with_cross_file_note(
            Span::new(100, 120),
            PathBuf::from("other/module.tg"),
            "return type declared here",
        );

    assert_eq!(err.notes.len(), 1);
    assert_eq!(
        err.notes[0].file_path.as_deref(),
        Some(Path::new("other/module.tg"))
    );
}

#[test]
fn trace_defaults_to_empty() {
    let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat);
    assert!(err.trace.is_empty());
}

#[test]
fn trace_frame_builder() {
    let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat)
        .with_trace_frame(Span::new(40, 50), PathBuf::from("caller.tg"), "call site")
        .with_trace_frame(
            Span::new(100, 110),
            PathBuf::from("callee.tg"),
            "return type",
        );

    assert_eq!(err.trace.len(), 2);
    assert_eq!(err.trace[0].file_path, PathBuf::from("caller.tg"));
    assert_eq!(err.trace[1].file_path, PathBuf::from("callee.tg"));
}

#[test]
fn serde_roundtrip_with_new_fields() {
    // AC6: serialization roundtrip for file_path in Note and trace frames
    let err = ElabError::type_mismatch(Span::new(0, 5), Type::Bool, Type::Nat)
        .with_cross_file_note(
            Span::new(100, 120),
            PathBuf::from("other.tg"),
            "note in other file",
        )
        .with_trace_frame(Span::new(40, 50), PathBuf::from("caller.tg"), "call site");

    let json = serde_json::to_string(&err).expect("serialize");
    let roundtripped: ElabError = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(roundtripped.notes.len(), 1);
    assert_eq!(
        roundtripped.notes[0].file_path.as_deref(),
        Some(Path::new("other.tg"))
    );
    assert_eq!(roundtripped.trace.len(), 1);
    assert_eq!(roundtripped.trace[0].file_path, PathBuf::from("caller.tg"));
}

#[test]
fn serde_backward_compat_missing_fields() {
    // AC6: JSON without file_path/trace deserializes with defaults
    let json = r#"{
        "message": "test",
        "span": {"start": 0, "end": 5},
        "kind": {"Other": "test"},
        "notes": [{"message": "n", "span": null}],
        "help": null,
        "context": null
    }"#;
    let err: ElabError = serde_json::from_str(json).expect("deserialize old format");
    assert!(err.file_path.is_none());
    assert!(err.trace.is_empty());
    assert!(err.notes[0].file_path.is_none());
}

#[test]
fn expected_context_file_path() {
    let ctx = ExpectedContext::return_type(Span::new(10, 20)).in_file("other/mod.tg");
    assert_eq!(ctx.file_path.as_deref(), Some(Path::new("other/mod.tg")));

    let ctx_no_file = ExpectedContext::return_type(Span::new(10, 20));
    assert!(ctx_no_file.file_path.is_none());
}
