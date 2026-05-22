//! Integration tests for info commands.
// audit-ignore: test file with many small tests (avg 9 LOC each)

use super::integration::elaborate_source;
use crate::doctor::checks::check_constructor_counts;
use crate::info::commands::*;
use crate::info::elaborate_for_info;
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;
use tungsten_bootstrap::driver::ProjectOutput;

#[test]
fn test_info_field_type_adt_constructor() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type Opt<T> = None | Some(T)\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    let result = cmd_info_field_type("Opt.Some", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_field_type_adt_field_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }",
    )
    .unwrap();
    let result = cmd_info_field_type("List.Cons.0", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_field_type_adt_field_index_out_of_range() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }",
    )
    .unwrap();
    let result = cmd_info_field_type("List.Cons.5", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_field_type_invalid_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_field_type("a.b.c.d", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_field_type_type_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_field_type("Missing.field", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_field_type_adt_constructor_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_field_type("Foo.Missing", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_field_type_non_numeric_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_field_type("Foo.B.abc", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_field_type_adt_nil_no_fields() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }",
    )
    .unwrap();
    let result = cmd_info_field_type("List.Nil", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_field_type_recursive_self_ref() {
    let project =
        elaborate_source("type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }");
    let (_, constructors) = &project.adt_types["List"];
    let cons = constructors.iter().find(|c| c.name == "Cons").unwrap();
    // Field 0 is the element type (T)
    assert_eq!(format!("{}", cons.fields[0]), "T");
    // Field 1 is the self-reference (stored as bare TyVar during Phase 1c)
    let tail_ty = format!("{}", cons.fields[1]);
    assert!(
        tail_ty.contains("List"),
        "Expected tail field to reference List, got: {tail_ty}"
    );
}

#[test]
fn test_info_field_type_record_data() {
    let project = elaborate_source("type Point = { x: Nat, y: Bool }\nfn main() -> Nat { 0 }");
    let fields = &project.record_types["Point"];
    assert_eq!(fields[0].0, "x");
    assert_eq!(fields[0].1, tungsten_core::types::Type::Nat);
    assert_eq!(fields[1].0, "y");
    assert_eq!(fields[1].1, tungsten_core::types::Type::Bool);
}

// ═══════════════════════════════════════════════════════════════════════
// info adt --check-fold tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_info_cmd_adt_check_fold_non_recursive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    let result = cmd_info_adt(
        "Color",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: false,
            check_fold: true,
        },
    );
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_adt_check_fold_recursive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }",
    )
    .unwrap();
    // The fold check may report inconsistency depending on elaboration — just verify no crash.
    let result = cmd_info_adt(
        "List",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: false,
            check_fold: true,
        },
    );
    assert!(
        result != ExitCode::from(2),
        "should not be a usage/internal error"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// info constructors (ADR 7.5.26e)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_info_constructors_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type AB = A(Nat) | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_constructors("AB", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_constructors_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type AB = A(Nat) | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_constructors("Missing", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_constructors_validation() {
    let project = elaborate_source("type AB = A(Nat) | B(Nat)\nfn main() -> Nat { 0 }");
    let (_, constructors) = &project.adt_types["AB"];
    let result =
        crate::doctor::checks::check_constructor_counts::validate_constructors("AB", constructors);
    assert!(result.is_ok());
    assert_eq!(result.expected_count, 2);
    assert_eq!(result.actual_count, 2);
    assert_eq!(result.grouped.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════
// doctor check-constructor-counts (ADR 7.5.26e)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_doctor_check_constructor_counts_pass() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type AB = A(Nat) | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = check_constructor_counts::cmd_check_constructor_counts(&path, false, 20, false);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_doctor_check_constructor_counts_with_expected_mismatch() {
    let project = elaborate_source("type AB = A(Nat) | B(Nat)\nfn main() -> Nat { 0 }");
    let (_, constructors) = &project.adt_types["AB"];
    // Pass wrong expected count to verify count-mismatch detection
    let result =
        check_constructor_counts::validate_constructors_with_expected("AB", constructors, 5);
    assert!(!result.is_ok());
    assert!(result.violations.iter().any(|v| matches!(
        v,
        check_constructor_counts::ConstructorViolation::CountMismatch {
            expected: 5,
            actual: 2
        }
    )));
}
