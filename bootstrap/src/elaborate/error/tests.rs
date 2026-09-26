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

// ─────────────────────────────────────────────────────────────────────────────
// The Other(String) / InternalError split (ADR 15.8.26b)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn internal_error_carries_its_own_code_and_framing() {
    let err = ElabError::internal(Span::new(0, 5), "the invariant that broke");
    assert_eq!(err.kind.code(), "E9998");
    assert!(err.message.contains("internal compiler error"));
    assert!(err.message.contains("the invariant that broke"));
    assert!(
        err.message.contains("please report it"),
        "the framing, not the site, asks for the bug report: {}",
        err.message
    );
}

/// The triage's residue is a CLOSED population: defensive branches behind
/// earlier passes, plus test-harness verdicts (`forms/builtins.rs`). A new
/// `Other` site should instead be a real kind (user-reachable condition) or
/// `ElabError::internal` (broken compiler invariant) — see ADR 15.8.26b.
#[test]
fn other_string_population_does_not_regrow() {
    let sites = other_construction_sites();
    assert!(
        sites.len() >= 10,
        "the census parsed only {} site(s) — the scan shape changed",
        sites.len()
    );
    assert!(
        sites.len() <= 18,
        "{} `Other(String)` construction sites, expected at most 18. A new \
         condition deserves a kind of its own, or `ElabError::internal` if no \
         user input can cause it (ADR 15.8.26b). Sites:\n{:#?}",
        sites.len(),
        sites.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
}

/// AC (ADR 15.8.26b): no site announces `internal error:` through the
/// uncoded catch-all — that class has its own kind now.
#[test]
fn no_other_site_says_internal_error() {
    for (path, snippet) in other_construction_sites() {
        assert!(
            !snippet.contains("internal error"),
            "`{path}` routes an internal-error message through Other(String) — \
             use ElabError::internal (E9998) instead:\n{snippet}"
        );
    }
}

/// `(file, snippet-after-the-call)` for every `Other` construction in
/// production elaborate/ code. Skips `tests` modules and `error/` itself
/// (where `Other` appears as pattern arms and the helper's own definition).
fn other_construction_sites() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("read elaborate/ source dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.is_dir() {
                if name != "tests" && name != "error" {
                    walk(&path, out);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" {
                let src = std::fs::read_to_string(&path).expect("read source file");
                for needle in ["ElabErrorKind::Other(", "ElabError::other("] {
                    for (at, _) in src.match_indices(needle) {
                        let end = (at + 250).min(src.len());
                        out.push((path.display().to_string(), src[at..end].to_string()));
                    }
                }
            }
        }
    }
    let root = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src/elaborate"));
    let mut sites = Vec::new();
    walk(&root, &mut sites);
    sites
}

/// Pins the census walker's snippet contract: each snippet is the text AT the
/// construction site (so the internal-error scan reads the right message) and
/// the window is bounded (so it cannot swallow unrelated later sites).
#[test]
fn census_snippets_are_bounded_windows_anchored_at_the_site() {
    for (path, snippet) in other_construction_sites() {
        assert!(
            snippet.starts_with("ElabErrorKind::Other(")
                || snippet.starts_with("ElabError::other("),
            "`{path}` snippet does not start at the construction site: {snippet:?}"
        );
        assert!(
            snippet.len() <= 250,
            "`{path}` snippet exceeds the 250-byte window ({} bytes)",
            snippet.len()
        );
    }
}
