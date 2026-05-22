use super::parsing::*;
use super::*;

use std::path::PathBuf;
use std::process::ExitCode;

#[test]
fn test_function_reset_after_closing_brace() {
    let ir = r#"
define void @first_fn() {
  ret void
}

  store { i32, [24 x i8] } %val, { i32, [8 x i8] }* %ptr
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 1);
    // Function should be None after the closing brace
    assert!(mismatches[0].function.is_none());
}

#[test]
fn test_line_numbers_correct() {
    let ir = "line 1\nline 2\n  store { i32, [24 x i8] } %v, { i32, [8 x i8] }* %p\nline 4\n";
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].line_number, 3);
}

// ═══════════════════════════════════════════════════════════════════
// extract_type_prefix / extract_ptr_target_type
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_extract_type_prefix_struct() {
    assert_eq!(
        extract_type_prefix("{ i32, i8 } %val"),
        Some("{ i32, i8 }".to_string())
    );
}

#[test]
fn test_extract_type_prefix_simple() {
    assert_eq!(extract_type_prefix("i32 %val"), Some("i32".to_string()));
}

#[test]
fn test_extract_ptr_target_type_struct() {
    assert_eq!(
        extract_ptr_target_type("{ i32, i8 }* %ptr"),
        Some("{ i32, i8 }".to_string())
    );
}

#[test]
fn test_extract_ptr_target_type_simple() {
    assert_eq!(
        extract_ptr_target_type("i32* %ptr"),
        Some("i32".to_string())
    );
}

// ═══════════════════════════════════════════════════════════════════
// split_struct_fields
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_split_struct_fields_simple() {
    let fields = split_struct_fields("i32, i8, i64");
    assert_eq!(fields, vec!["i32", " i8", " i64"]);
}

#[test]
fn test_split_struct_fields_nested() {
    let fields = split_struct_fields("i32, { i8, i8 }, i64");
    assert_eq!(fields, vec!["i32", " { i8, i8 }", " i64"]);
}

#[test]
fn test_split_struct_fields_single() {
    let fields = split_struct_fields("i32");
    assert_eq!(fields, vec!["i32"]);
}

// ═══════════════════════════════════════════════════════════════════
// find_top_level_comma — mismatched braces
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_find_top_level_comma_leading_close_brace() {
    // Leading `}` drives depth negative, so the comma at depth -1 is skipped
    assert_eq!(find_top_level_comma("} foo, bar"), None);
}

#[test]
fn test_find_top_level_comma_unbalanced_open() {
    // Unclosed brace — comma is at depth 1, never found at depth 0
    assert_eq!(find_top_level_comma("{ foo, bar"), None);
}

#[test]
fn test_find_top_level_comma_recovery_after_mismatch() {
    // } drops depth to -1, { raises to 0, so the comma is at depth 0
    assert_eq!(find_top_level_comma("} { foo, bar"), Some(7));
}

// ═══════════════════════════════════════════════════════════════════
// extract_ptr_target_type — edge cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_extract_ptr_target_type_struct_without_star() {
    // Struct without `*` — not a pointer, should return None
    assert_eq!(extract_ptr_target_type("{ i32, i8 } %ptr"), None);
}

#[test]
fn test_extract_ptr_target_type_empty() {
    assert_eq!(extract_ptr_target_type(""), None);
}

#[test]
fn test_extract_ptr_target_type_star_only() {
    // Just `*` with no type before it
    assert_eq!(extract_ptr_target_type("* %ptr"), None);
}

// ═══════════════════════════════════════════════════════════════════
// extract_type_prefix — edge cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_extract_type_prefix_empty() {
    assert_eq!(extract_type_prefix(""), None);
}

#[test]
fn test_extract_type_prefix_only_operand() {
    // No space → rfind(' ') returns None
    assert_eq!(extract_type_prefix("%val"), None);
}

#[test]
fn test_extract_type_prefix_unclosed_brace() {
    // Unclosed brace → find_matching_brace returns None
    assert_eq!(extract_type_prefix("{ i32, i8 %val"), None);
}

// ═══════════════════════════════════════════════════════════════════
// format_json — special characters in instructions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_format_json_special_chars_in_instruction() {
    use super::parsing::StoreMismatch;

    let mismatches = vec![StoreMismatch {
        line_number: 1,
        instruction: "store i8 \"hello\\world\"\tnewline\nhere".to_string(),
        function: Some("test_fn".to_string()),
        value_size: 8,
        pointer_size: 4,
    }];
    let path = PathBuf::from("test.ll");
    let json = super::format_json(&path, &mismatches);

    // The JSON should be parseable
    assert!(json.contains("\\\\"), "backslash should be double-escaped");
    assert!(json.contains("\\n"), "newline should be escaped");
    assert!(json.contains("\\t"), "tab should be escaped");
    // Verify it doesn't contain raw control characters
    assert!(!json.contains('\t'), "raw tab should not appear in output");
    assert_eq!(
        json.matches('\n').count(),
        2,
        "only structural newlines in JSON format"
    );
}

#[test]
fn test_format_json_quotes_in_instruction() {
    use super::parsing::StoreMismatch;

    let mismatches = vec![StoreMismatch {
        line_number: 5,
        instruction: "store i8* getelementptr(\"hello\")".to_string(),
        function: None,
        value_size: 8,
        pointer_size: 1,
    }];
    let path = PathBuf::from("test.ll");
    let json = super::format_json(&path, &mismatches);

    assert!(
        json.contains("\\\"hello\\\""),
        "quotes in instruction should be escaped"
    );
    assert!(
        json.contains("\"function\": null"),
        "null function should appear"
    );
}

#[test]
fn test_format_json_empty_mismatches() {
    let path = PathBuf::from("clean.ll");
    let json = super::format_json(&path, &[]);

    assert!(json.contains("\"status\": \"pass\""));
    assert!(json.contains("\"mismatches\": ["));
}

// ═══════════════════════════════════════════════════════════════════
// find_store_load_mismatches — store with no comma (malformed)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_malformed_store_no_comma() {
    let ir = "define void @foo() {\n  store i32 42\n  ret void\n}\n";
    let mismatches = find_store_load_mismatches(ir);
    assert!(
        mismatches.is_empty(),
        "malformed store without comma should not crash"
    );
}

#[test]
fn test_malformed_store_empty_operands() {
    let ir = "define void @foo() {\n  store ,\n  ret void\n}\n";
    let mismatches = find_store_load_mismatches(ir);
    assert!(
        mismatches.is_empty(),
        "store with empty operands should not crash"
    );
}
