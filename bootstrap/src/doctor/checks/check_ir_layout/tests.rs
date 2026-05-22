use super::parsing::*;
use super::*;

use std::path::PathBuf;
use std::process::ExitCode;

#[test]
fn test_estimate_struct_size_integer() {
    assert_eq!(estimate_struct_size("i8"), 1);
    assert_eq!(estimate_struct_size("i32"), 4);
    assert_eq!(estimate_struct_size("i64"), 8);
}

#[test]
fn test_estimate_struct_size_array() {
    assert_eq!(estimate_struct_size("[24 x i8]"), 24);
    assert_eq!(estimate_struct_size("[4 x i32]"), 16);
}

#[test]
fn test_estimate_struct_size_struct() {
    assert_eq!(estimate_struct_size("{ i32, [24 x i8] }"), 28);
    assert_eq!(estimate_struct_size("{ i32, [8 x i8] }"), 12);
}

#[test]
fn test_estimate_struct_size_unknown() {
    assert_eq!(estimate_struct_size("i32*"), 0);
    assert_eq!(estimate_struct_size("%MyType"), 0);
}

#[test]
fn test_find_mismatch_in_ir() {
    let ir = r#"
define void @MaybeTypeExpr_constructor() {
  store { i32, [24 x i8] } %val, { i32, [8 x i8] }* %ptr
  ret void
}
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].value_size, 28);
    assert_eq!(mismatches[0].pointer_size, 12);
    assert_eq!(
        mismatches[0].function.as_deref(),
        Some("MaybeTypeExpr_constructor")
    );
}

#[test]
fn test_no_mismatch_consistent_store() {
    let ir = r#"
define void @foo() {
  store { i32, [8 x i8] } %val, { i32, [8 x i8] }* %ptr
  ret void
}
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert!(mismatches.is_empty());
}

#[test]
fn test_no_mismatch_simple_types() {
    let ir = r#"
define void @bar() {
  store i32 42, i32* %ptr
  ret void
}
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert!(mismatches.is_empty());
}

#[test]
fn test_cmd_missing_file() {
    let result = cmd_check_ir_layout(&PathBuf::from("/nonexistent/file.ll"), false);
    assert_eq!(result, ExitCode::from(2));
}

#[test]
fn test_cmd_json_output() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.ll");
    std::fs::write(
        &path,
        "define void @foo() {\n  store i32 42, i32* %ptr\n  ret void\n}\n",
    )
    .unwrap();
    let result = cmd_check_ir_layout(&path, true);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_cmd_json_output_with_mismatch() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.ll");
    std::fs::write(
        &path,
        "define void @foo() {\n  store { i32, [24 x i8] } %v, { i32, [8 x i8] }* %p\n  ret void\n}\n",
    )
    .unwrap();
    let result = cmd_check_ir_layout(&path, true);
    assert_eq!(result, ExitCode::FAILURE);
}

// ═══════════════════════════════════════════════════════════════════
// estimate_struct_size edge cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_estimate_struct_size_nested_struct() {
    // { i32, { i8, i8 } } = 4 + (1 + 1) = 6
    assert_eq!(estimate_struct_size("{ i32, { i8, i8 } }"), 6);
}

#[test]
fn test_estimate_struct_size_deeply_nested() {
    // { { i32, i32 }, { i8, i8 } } = (4+4) + (1+1) = 10
    assert_eq!(estimate_struct_size("{ { i32, i32 }, { i8, i8 } }"), 10);
}

#[test]
fn test_estimate_struct_size_single_field() {
    assert_eq!(estimate_struct_size("{ i64 }"), 8);
}

#[test]
fn test_estimate_struct_size_i1() {
    assert_eq!(estimate_struct_size("i1"), 1);
}

#[test]
fn test_estimate_struct_size_i16() {
    assert_eq!(estimate_struct_size("i16"), 2);
}

#[test]
fn test_estimate_struct_size_array_of_struct() {
    // [2 x { i32, i32 }] = 2 * (4+4) = 16
    assert_eq!(estimate_struct_size("[2 x { i32, i32 }]"), 16);
}

#[test]
fn test_estimate_struct_size_empty_struct() {
    assert_eq!(estimate_struct_size("{ }"), 0);
}

#[test]
fn test_estimate_struct_size_struct_with_unknown_field() {
    // Unknown field type → returns 0
    assert_eq!(estimate_struct_size("{ i32, %MyType }"), 0);
}

// ═══════════════════════════════════════════════════════════════════
// find_top_level_comma
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_find_top_level_comma_simple() {
    assert_eq!(find_top_level_comma("a, b"), Some(1));
}

#[test]
fn test_find_top_level_comma_nested_braces() {
    // Comma inside braces should be skipped
    assert_eq!(
        find_top_level_comma("{ i32, i8 } %val, { i32, i8 }* %ptr"),
        Some(16)
    );
}

#[test]
fn test_find_top_level_comma_no_comma() {
    assert_eq!(find_top_level_comma("just a string"), None);
}

#[test]
fn test_find_top_level_comma_nested_brackets() {
    assert_eq!(find_top_level_comma("[2 x i8] %val, i32* %ptr"), Some(13));
}

// ═══════════════════════════════════════════════════════════════════
// IR mismatch scenarios
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_multiple_mismatches_in_one_file() {
    let ir = r#"
define void @foo() {
  store { i32, [24 x i8] } %a, { i32, [8 x i8] }* %p1
  store { i64, [16 x i8] } %b, { i64, [8 x i8] }* %p2
  ret void
}
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 2);
    assert_eq!(mismatches[0].value_size, 28);
    assert_eq!(mismatches[0].pointer_size, 12);
    assert_eq!(mismatches[1].value_size, 24);
    assert_eq!(mismatches[1].pointer_size, 16);
}

#[test]
fn test_store_outside_function() {
    // Store before any define — function should be None
    let ir = "  store { i32, [24 x i8] } %val, { i32, [8 x i8] }* %ptr\n";
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 1);
    assert!(mismatches[0].function.is_none());
}

#[test]
fn test_function_tracking_across_functions() {
    let ir = r#"
define void @first_fn() {
  store i32 42, i32* %ptr
  ret void
}

define void @second_fn() {
  store { i32, [24 x i8] } %val, { i32, [8 x i8] }* %ptr
  ret void
}
"#;
    let mismatches = find_store_load_mismatches(ir);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].function.as_deref(), Some("second_fn"));
}
