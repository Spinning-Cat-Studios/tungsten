//! Tests for hint helper functions: formatting, shell safety, type extraction, JSON output.

use super::*;
use crate::ElabErrorKind;
use tungsten_core::Type;

// ─────────────────────────────────────────────────────────────────────────────
// Format
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_format_hints_output() {
    let hints = vec![DiagnosticHint {
        command: "tungsten info type-encoding Nat test.tg".to_string(),
        reason: "reason".to_string(),
    }];
    let output = format_hints(&hints);
    assert!(output.contains("hint: run `tungsten info type-encoding Nat test.tg`"));
}

#[test]
fn test_format_hints_empty() {
    let hints: Vec<DiagnosticHint> = vec![];
    let output = format_hints(&hints);
    assert!(output.is_empty());
}

#[test]
fn test_format_hints_two_hints() {
    let hints = vec![
        DiagnosticHint {
            command: "tungsten info type-encoding List test.tg".to_string(),
            reason: "reason 1".to_string(),
        },
        DiagnosticHint {
            command: "tungsten doctor suggest-tools \"type mismatch\"".to_string(),
            reason: "reason 2".to_string(),
        },
    ];
    let output = format_hints(&hints);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("info type-encoding"));
    assert!(lines[1].contains("suggest-tools"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Shell safety
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_shell_safe_path_simple() {
    let path = std::path::Path::new("examples/list.tg");
    assert_eq!(shell_safe_path(path), "examples/list.tg");
}

#[test]
fn test_shell_safe_path_with_spaces() {
    let path = std::path::Path::new("my project/list.tg");
    let safe = shell_safe_path(path);
    assert!(safe.starts_with('\''));
    assert!(safe.ends_with('\''));
}

#[test]
fn test_shell_safe_path_with_dollar() {
    let path = std::path::Path::new("$HOME/list.tg");
    let safe = shell_safe_path(path);
    assert_eq!(safe, "'$HOME/list.tg'");
}

#[test]
fn test_shell_safe_path_with_backslash() {
    let path = std::path::Path::new("path\\to\\list.tg");
    let safe = shell_safe_path(path);
    assert!(safe.starts_with('\''));
    assert!(safe.ends_with('\''));
}

#[test]
fn test_shell_safe_path_with_single_quote() {
    let path = std::path::Path::new("it's/list.tg");
    let safe = shell_safe_path(path);
    // Single quotes in the path are escaped: 'it'\''s/list.tg'
    assert!(
        safe.contains("'\\''"),
        "Expected escaped single quote, got: {safe}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// extract_type_name
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_extract_type_name_app() {
    let ty = Type::App("Option".to_string(), vec![Type::Nat]);
    assert_eq!(extract_type_name(&ty), Some("Option".to_string()));
}

#[test]
fn test_extract_type_name_adt() {
    let ty = Type::Adt("List".to_string(), vec![Type::Nat], vec![]);
    assert_eq!(extract_type_name(&ty), Some("List".to_string()));
}

#[test]
fn test_extract_type_name_mu_recurses() {
    let inner = Type::App("Tree".to_string(), vec![]);
    let ty = Type::Mu("α".to_string(), Box::new(inner));
    assert_eq!(extract_type_name(&ty), Some("Tree".to_string()));
}

#[test]
fn test_extract_type_name_primitive_returns_none() {
    assert_eq!(extract_type_name(&Type::Nat), None);
    assert_eq!(extract_type_name(&Type::Bool), None);
    assert_eq!(extract_type_name(&Type::String), None);
}

#[test]
fn test_extract_type_name_arrow_returns_none() {
    let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    assert_eq!(extract_type_name(&ty), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// compute_line_number
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_compute_line_number_first_line() {
    assert_eq!(compute_line_number("hello\nworld\n", 0), 1);
}

#[test]
fn test_compute_line_number_second_line() {
    assert_eq!(compute_line_number("hello\nworld\n", 6), 2);
}

#[test]
fn test_compute_line_number_past_end_clamps() {
    // Should not panic, clamps to source length
    assert_eq!(compute_line_number("hello\n", 999), 2);
}

#[test]
fn test_compute_line_number_empty_source() {
    assert_eq!(compute_line_number("", 0), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// error_to_json
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_to_json_structure() {
    use crate::span::Span;
    use crate::ElabError;

    let error = ElabError::new(
        Span { start: 15, end: 22 },
        ElabErrorKind::TypeMismatch {
            expected: Type::Adt("List".to_string(), vec![Type::Nat], vec![]),
            found: Type::Bool,
        },
    )
    .with_file_path(std::path::PathBuf::from("examples/list.tg"));

    // "fn main() -> X {\n    Cons(1, Nil)\n}\n"
    //  offset 15 = 'X', still on line 1 (newline is at index 17)
    let source = "fn main() -> X {\n    Cons(1, Nil)\n}\n";
    let json = error_to_json(&error, source);

    assert_eq!(json.code, "E0010");
    assert_eq!(json.file, Some("examples/list.tg".to_string()));
    assert_eq!(json.line, Some(1)); // offset 15 is on line 1
    assert!(!json.hints.is_empty());
    assert!(json.hints[0].command.contains("List"));
    assert!(!json.hints[0].reason.is_empty());
}

#[test]
fn test_json_diagnostic_report_round_trip() {
    use crate::span::Span;
    use crate::ElabError;

    let errors = vec![
        ElabError::new(
            Span { start: 0, end: 5 },
            ElabErrorKind::TypeMismatch {
                expected: Type::Nat,
                found: Type::Bool,
            },
        )
        .with_file_path(std::path::PathBuf::from("test.tg")),
        ElabError::new(
            Span { start: 10, end: 15 },
            ElabErrorKind::UndefinedVariable("x".to_string()),
        ),
    ];

    let source = "let a = true\nlet b = x\n";
    let report = JsonDiagnosticReport {
        errors: errors.iter().map(|e| error_to_json(e, source)).collect(),
    };

    let json_str = serde_json::to_string(&report).expect("serialization should not fail");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).expect("deserialization should not fail");

    // Top-level structure
    assert!(parsed.is_object());
    let err_array = parsed["errors"]
        .as_array()
        .expect("errors should be an array");
    assert_eq!(err_array.len(), 2);

    // First error: has file path
    assert_eq!(err_array[0]["code"].as_str().unwrap(), "E0010");
    assert_eq!(err_array[0]["file"].as_str().unwrap(), "test.tg");
    assert_eq!(err_array[0]["line"].as_u64().unwrap(), 1);
    assert!(err_array[0]["hints"].as_array().unwrap().len() > 0);

    // Second error: no file path → "file" key absent (skip_serializing_if)
    assert!(
        err_array[1]["file"].is_null(),
        "missing file should serialize as null or be absent"
    );

    // Hint structure
    let hint = &err_array[0]["hints"][0];
    assert!(hint["command"].is_string());
    assert!(hint["reason"].is_string());
}
