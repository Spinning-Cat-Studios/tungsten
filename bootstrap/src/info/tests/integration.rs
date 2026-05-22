//! Integration tests for info commands.
// audit-ignore: test file with many small tests (avg 9 LOC each)

use crate::doctor::checks::check_constructor_counts;
use crate::info::commands::*;
use crate::info::elaborate_for_info;
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;
use tungsten_bootstrap::driver::ProjectOutput;

/// Helper: create a temp .tg file and elaborate it for info.
pub(super) fn elaborate_source(source: &str) -> ProjectOutput {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, source).unwrap();
    elaborate_for_info(&path, false, 20).expect("elaboration should succeed")
}

#[test]
fn test_info_types_adt() {
    let project = elaborate_source("type Option<T> = None | Some(T)\nfn main() -> Nat { 0 }");
    assert!(project.adt_types.contains_key("Option"));
    let (params, ctors) = &project.adt_types["Option"];
    assert_eq!(params, &["T"]);
    assert_eq!(ctors.len(), 2);
    assert_eq!(ctors[0].name, "None");
    assert_eq!(ctors[1].name, "Some");
}

#[test]
fn test_info_types_record() {
    let project = elaborate_source("type Point = { x: Nat, y: Nat }\nfn main() -> Nat { 0 }");
    assert!(project.record_types.contains_key("Point"));
    let fields = &project.record_types["Point"];
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "x");
    assert_eq!(fields[1].0, "y");
}

#[test]
fn test_info_types_alias() {
    let project = elaborate_source("type MyNat = Nat\nfn main() -> Nat { 0 }");
    assert!(project.type_aliases.contains_key("MyNat"));
    let (params, target) = &project.type_aliases["MyNat"];
    assert!(params.is_empty());
    assert_eq!(*target, tungsten_core::types::Type::Nat);
}

#[test]
fn test_info_def_found() {
    let project = elaborate_source("fn answer() -> Nat { 42 }\nfn main() -> Nat { 0 }");
    assert!(project.defs.iter().any(|d| d.name == "answer"));
    let answer = project.defs.iter().find(|d| d.name == "answer").unwrap();
    assert_eq!(answer.ty, tungsten_core::types::Type::Nat);
}

#[test]
fn test_info_provenance_for_recursive_adt() {
    let project = elaborate_source(
        r#"
type List<T> = Nil | Cons(T, List<T>)
fn main() -> List<Nat> { Nil() }
"#,
    );
    assert!(project.adt_types.contains_key("List"));
    let has_list_origin = project
        .type_provenance
        .mu_origins
        .values()
        .any(|o| o.adt_name == "List");
    assert!(has_list_origin, "Expected provenance for List ADT");
}

#[test]
fn test_info_cmd_types_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_types(&path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_adt_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_adt(
        "Foo",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: false,
            check_fold: false,
        },
    );
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_adt_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_adt(
        "NonExistent",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: false,
            check_fold: false,
        },
    );
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_cmd_def_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "fn answer() -> Nat { 42 }").unwrap();
    let result = cmd_info_def("answer", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_def_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "fn answer() -> Nat { 42 }").unwrap();
    let result = cmd_info_def("missing", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}

#[test]
fn test_info_cmd_encoding_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_encoding("Foo", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_pipeline_exit_success() {
    let result = cmd_info_pipeline();
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_types_no_types() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "fn main() -> Nat { 42 }").unwrap();
    let result = cmd_info_types(&path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_multiple_type_kinds() {
    let project = elaborate_source(
        r#"
type Color = Red | Green | Blue
type Point = { x: Nat, y: Nat }
type Count = Nat
fn main() -> Nat { 0 }
"#,
    );
    assert!(project.adt_types.contains_key("Color"));
    assert!(project.record_types.contains_key("Point"));
    assert!(project.type_aliases.contains_key("Count"));
}

// ═══════════════════════════════════════════════════════════════════════
// info adt --show-fields tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_info_cmd_adt_show_fields_exit_success() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, "type Foo = A | B(Nat)\nfn main() -> Nat { 0 }").unwrap();
    let result = cmd_info_adt(
        "Foo",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: true,
            check_fold: false,
        },
    );
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_cmd_adt_show_fields_recursive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type List<T> = Nil | Cons(T, List<T>)\nfn main() -> List<Nat> { Nil() }",
    )
    .unwrap();
    let result = cmd_info_adt(
        "List",
        &path,
        &AdtInfoOptions {
            verbose: false,
            max_errors: 20,
            show_fields: true,
            check_fold: false,
        },
    );
    assert_eq!(result, ExitCode::SUCCESS);
}

// ═══════════════════════════════════════════════════════════════════════
// info field-type tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_info_field_type_record() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type Point = { x: Nat, y: Nat }\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    let result = cmd_info_field_type("Point.x", &path, false, 20);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn test_info_field_type_record_field_not_found() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(
        &path,
        "type Point = { x: Nat, y: Nat }\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    let result = cmd_info_field_type("Point.z", &path, false, 20);
    assert_eq!(result, ExitCode::FAILURE);
}
