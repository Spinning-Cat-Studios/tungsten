//! Tests for `tungsten doctor check declares`.

use super::scanner::find_missing_declarations;

/// A well-formed IR file with all calls declared — should produce no errors.
const VALID_IR: &str = "\
declare ptr @malloc(i64)
declare void @free(ptr)

define i64 @main$direct(ptr %0) {
entry:
  %1 = call ptr @malloc(i64 16)
  call void @free(ptr %1)
  %2 = call i64 @helper$direct(ptr %0)
  ret i64 %2
}

define i64 @helper$direct(ptr %0) {
entry:
  ret i64 42
}
";

/// IR with a missing declaration — should flag the call to @missing_fn.
const BROKEN_IR: &str = "\
declare ptr @malloc(i64)

define i64 @main$direct(ptr %0) {
entry:
  %1 = call ptr @malloc(i64 16)
  %2 = call i64 @missing_fn(ptr %0)
  ret i64 %2
}
";

/// IR containing LLVM intrinsics — should NOT flag them.
const INTRINSIC_IR: &str = "\
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
declare void @llvm.lifetime.start.p0(i64, ptr)

define void @copy_helper(ptr %dst, ptr %src) {
entry:
  call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr %src, i64 8, i1 false)
  call void @llvm.lifetime.start.p0(i64 8, ptr %dst)
  ret void
}
";

/// IR with indirect calls through function pointers — should NOT flag them.
const INDIRECT_CALL_IR: &str = "\
define void @call_through_ptr(ptr %fptr) {
entry:
  call void %fptr()
  ret void
}
";

#[test]
fn valid_ir_no_missing() {
    let missing = find_missing_declarations(VALID_IR);
    assert!(
        missing.is_empty(),
        "expected no missing declarations, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

#[test]
fn broken_ir_reports_missing_symbol() {
    let missing = find_missing_declarations(BROKEN_IR);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].symbol, "missing_fn");
    // Line 6: `%2 = call i64 @missing_fn(ptr %0)`
    assert_eq!(missing[0].line_number, 6);
}

#[test]
fn intrinsics_are_ignored() {
    let missing = find_missing_declarations(INTRINSIC_IR);
    assert!(
        missing.is_empty(),
        "LLVM intrinsics should not be flagged, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

#[test]
fn indirect_calls_are_ignored() {
    let missing = find_missing_declarations(INDIRECT_CALL_IR);
    assert!(
        missing.is_empty(),
        "indirect calls should not be flagged, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

#[test]
fn exit_code_failure_on_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let ll_path = dir.path().join("broken.ll");
    std::fs::write(&ll_path, BROKEN_IR).unwrap();

    let result = super::cmd_check_declares(dir.path());
    assert_eq!(result, std::process::ExitCode::FAILURE);
}

#[test]
fn exit_code_success_on_valid() {
    let dir = tempfile::TempDir::new().unwrap();
    let ll_path = dir.path().join("valid.ll");
    std::fs::write(&ll_path, VALID_IR).unwrap();

    let result = super::cmd_check_declares(dir.path());
    assert_eq!(result, std::process::ExitCode::SUCCESS);
}

#[test]
fn multiple_files_scanned() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.ll"), VALID_IR).unwrap();
    std::fs::write(dir.path().join("b.ll"), BROKEN_IR).unwrap();

    let result = super::cmd_check_declares(dir.path());
    assert_eq!(result, std::process::ExitCode::FAILURE);
}

#[test]
fn empty_directory_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let result = super::cmd_check_declares(dir.path());
    assert_eq!(result, std::process::ExitCode::FAILURE);
}
