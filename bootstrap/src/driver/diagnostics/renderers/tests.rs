//! Unit tests for the diagnostic renderers (split out of `mod.rs` to keep it
//! under the file-size threshold; ADR 4.7.26c follow-up).

use super::*;
use crate::elaborate::TraceFrame;
use crate::span::Span;
use std::path::PathBuf;

#[test]
fn byte_offset_to_line_no_source_map() {
    // When no source map is available, byte_offset_to_line returns the raw offset
    let result = byte_offset_to_line(None, Path::new("missing.tg"), 42);
    assert_eq!(result, "42");
}

#[test]
fn no_main_hint_points_at_cache_not_file_path() {
    // ADR 4.7.26c: NoMainFunction's synthetic EOF span must NOT be blamed on
    // file_path tracking (the superseded 3.7.26f mis-diagnosis); it points
    // at the elab cache instead.
    let hint = out_of_bounds_hint(&crate::ElabErrorKind::NoMainFunction);
    assert!(hint.contains("cache clean"));
    assert!(!hint.contains("file_path"));
}

#[test]
fn other_kinds_keep_file_path_hint() {
    // A genuine out-of-bounds span for any other kind keeps the file_path
    // tracking hint, which is accurate there.
    let hint = out_of_bounds_hint(&crate::ElabErrorKind::NonExhaustiveMatch);
    assert!(hint.contains("file_path tracking"));
}

#[test]
fn byte_offset_to_line_with_source() {
    let mut sm = SourceMap::new();
    sm.insert(
        PathBuf::from("test.tg"),
        "line1\nline2\nline3\n".to_string(),
    );
    let result = byte_offset_to_line(Some(&sm), Path::new("test.tg"), 6); // first char of line2
    assert_eq!(result, "2");
}

#[test]
fn format_trace_note_empty() {
    assert!(format_trace_note(&[], None).is_none());
}

#[test]
fn format_trace_note_without_source_map_uses_offsets() {
    let trace = vec![TraceFrame {
        message: "call site".to_string(),
        span: Span::new(42, 50),
        file_path: PathBuf::from("caller.tg"),
    }];
    let result = format_trace_note(&trace, None).unwrap();
    assert!(result.contains("caller.tg:42"));
    assert!(result.contains("call site"));
}

#[test]
fn format_trace_note_with_source_map_uses_lines() {
    let mut sm = SourceMap::new();
    sm.insert(
        PathBuf::from("caller.tg"),
        "fn main() =\n  helper()\n".to_string(),
    );
    let trace = vec![TraceFrame {
        message: "call site".to_string(),
        span: Span::new(14, 22), // "helper()" on line 2
        file_path: PathBuf::from("caller.tg"),
    }];
    let result = format_trace_note(&trace, Some(&sm)).unwrap();
    assert!(result.contains("caller.tg:2"));
}

// ── identical-render E0010 enrichment reaches both renderers (ADR 21.7.26f) ──
//
// These drive the renderers for their side effect on stderr; the assertion that
// matters is that neither path panics and both accept an enriched error, so the
// note wiring cannot silently rot. The note's *content* is asserted as a pure
// function in `tests_structural_divergence.rs`.

/// A `StrMap<CtorBucket>` mismatch whose two sides render identically.
fn identically_rendering_mismatch(span: Span) -> ElabError {
    use tungsten_core::Type;
    let resolved = Type::App(
        "StrMap".to_string(),
        vec![Type::Adt(
            "CtorBucket".to_string(),
            vec![],
            vec![("Bucket".to_string(), Type::Nat)],
        )],
    );
    let unresolved = Type::App(
        "StrMap".to_string(),
        vec![Type::App("CtorBucket".to_string(), vec![])],
    );
    ElabError::type_mismatch(span, resolved, unresolved)
}

#[test]
fn ariadne_renderer_accepts_an_enriched_mismatch() {
    let source = "fn main() -> Nat { 0 }";
    let error = identically_rendering_mismatch(Span::new(0, 10));
    render_elab_error(source, "main.tg", &error, None, None);
}

#[test]
fn fallback_renderer_accepts_an_enriched_mismatch() {
    // An out-of-bounds span forces the plain-text fallback path, which must
    // carry the same enrichment.
    let error = identically_rendering_mismatch(Span::new(9_000, 9_010));
    render_elab_error_fallback("main.tg", &error);
}

// ─────────────────────────────────────────────────────────────────────────────
// Note combining (ADR 8.8.26d)
// ─────────────────────────────────────────────────────────────────────────────
//
// ariadne's `Report::with_note` overwrites rather than appends. The renderer
// therefore gathers every span-less note and sets one; these pin that folding,
// because the symptom of getting it wrong is silent — notes simply vanish from
// the rendered block while remaining present on the `ElabError`.

#[test]
fn no_notes_yields_no_note_block() {
    assert_eq!(super::combine_notes(&[]), None);
}

#[test]
fn one_note_is_passed_through_unchanged() {
    let notes = vec!["strict positivity is required".to_string()];
    assert_eq!(
        super::combine_notes(&notes),
        Some("strict positivity is required".to_string())
    );
}

#[test]
fn every_note_survives_the_fold() {
    // The regression: before ADR 8.8.26d each `with_note` call replaced the
    // last, so an error with three notes rendered only the third.
    let notes = vec![
        "`Fn1` does not use its parameter `T` strictly positively".to_string(),
        "strict positivity is required for structural recursion".to_string(),
        "elaboration trace: …".to_string(),
    ];
    let combined = super::combine_notes(&notes).expect("three notes combine");
    for note in &notes {
        assert!(
            combined.contains(note),
            "dropped `{note}` from {combined:?}"
        );
    }
}

#[test]
fn notes_are_separated_so_they_read_as_distinct_statements() {
    let notes = vec!["first".to_string(), "second".to_string()];
    assert_eq!(
        super::combine_notes(&notes),
        Some("first\nsecond".to_string())
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Byte-indexed spans (ADR 24.9.26b)
// ─────────────────────────────────────────────────────────────────────────────
//
// Spans are byte offsets; ariadne's default reads them as characters, so each
// multi-byte character before a span shifted its label and heading right.

/// Render a one-label warning through the factory, ANSI stripped the way the
/// golden harness strips it.
fn render_single_label(source: &str, span: std::ops::Range<usize>) -> String {
    let mut rendered_bytes = Vec::new();
    super::report_factory::build_report(ReportKind::Warning, "f.tg", span.start)
        .with_message("probe")
        .with_label(Label::new(("f.tg", span)).with_message("here"))
        .finish()
        .write(("f.tg", Source::from(source)), &mut rendered_bytes)
        .unwrap();
    strip_ansi(&String::from_utf8(rendered_bytes).unwrap())
}

fn strip_ansi(text: &str) -> String {
    let mut plain = String::new();
    let mut in_escape = false;
    for c in text.chars() {
        match (in_escape, c) {
            (false, '\x1b') => in_escape = true,
            (true, 'm') => in_escape = false,
            (true, _) => {}
            (false, _) => plain.push(c),
        }
    }
    plain
}

/// The character column of the `┬` under `token` in its rendered source row,
/// and the character column of `token` in that row — equal when the label
/// lands on its token.
fn marker_and_token_columns(rendered: &str, source_line: &str, token: &str) -> (usize, usize) {
    let lines: Vec<&str> = rendered.lines().collect();
    let row = lines
        .iter()
        .position(|l| l.ends_with(source_line))
        .unwrap_or_else(|| panic!("no row for {source_line:?} in:\n{rendered}"));
    let token_byte = lines[row].rfind(token).unwrap();
    let token_col = lines[row][..token_byte].chars().count();
    let marker_col = lines[row + 1]
        .chars()
        .position(|c| c == '┬')
        .unwrap_or_else(|| panic!("no marker under {source_line:?} in:\n{rendered}"));
    (marker_col, token_col)
}

// 24.9.26b AC1: a multi-byte character on an EARLIER line.
#[test]
fn multibyte_on_an_earlier_line_leaves_label_on_its_token() {
    let source = "// a — b\nlet x = y;\n";
    let start = source.find("y;").unwrap();
    let rendered = render_single_label(source, start..start + 1);
    assert!(rendered.contains("[f.tg:2:9]"), "heading:\n{rendered}");
    let (marker, token) = marker_and_token_columns(&rendered, "let x = y;", "y");
    // The five-character gutter ` 2 │ ` plus `y`'s 0-based character column.
    assert_eq!(token, 5 + 8, "token column:\n{rendered}");
    assert_eq!(marker, token, "label off its token:\n{rendered}");
}

// 24.9.26b AC1: a multi-byte character on the SAME line — the case that tells
// a byte heading column from a character one.
#[test]
fn multibyte_on_the_same_line_counts_the_heading_in_characters() {
    let source = "let s = \"é—\"; z\n";
    let start = source.rfind('z').unwrap();
    let rendered = render_single_label(source, start..start + 1);
    // Byte column 18, character column 15.
    assert!(rendered.contains("[f.tg:1:15]"), "heading:\n{rendered}");
    let (marker, token) = marker_and_token_columns(&rendered, "let s = \"é—\"; z", "z");
    // The five-character gutter ` 1 │ ` plus `z`'s 0-based character column.
    assert_eq!(token, 5 + 14, "token column:\n{rendered}");
    assert_eq!(marker, token, "label off its token:\n{rendered}");
}

// 24.9.26b AC2: the factory is the only door. Every report elsewhere would
// silently fall back to ariadne's character indexing.
#[test]
fn no_report_is_built_outside_the_factory() {
    let needle = concat!("Report", "::build(");
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut pending = vec![src_root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.ends_with("renderers/report_factory.rs")
                && std::fs::read_to_string(&path).unwrap().contains(needle)
            {
                offenders.push(path);
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "direct report builders outside report_factory.rs: {offenders:?}"
    );
}
