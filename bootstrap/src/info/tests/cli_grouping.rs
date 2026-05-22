//! CLI grouping tests — verify both grouped and legacy paths parse (ADR 12.5.26h).

use crate::info::InfoCommands;
use clap::Parser;
/// Minimal wrapper to parse `InfoCommands` as if from the CLI.
#[derive(Parser)]
struct TestCli {
    #[command(subcommand)]
    cmd: InfoCommands,
}

fn parse(args: &[&str]) -> Result<TestCli, clap::Error> {
    TestCli::try_parse_from(std::iter::once("test").chain(args.iter().copied()))
}

// ── Grouped paths ──

#[test]
fn test_info_type_types_parses() {
    assert!(parse(&["type", "types", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_adt_parses() {
    assert!(parse(&["type", "adt", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_adt_with_flags_parses() {
    assert!(parse(&[
        "type",
        "adt",
        "List",
        "test.tg",
        "--show-fields",
        "--check-fold"
    ])
    .is_ok());
}

#[test]
fn test_info_type_encoding_parses() {
    assert!(parse(&["type", "encoding", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_type_encoding_parses() {
    assert!(parse(&["type", "type-encoding", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_type_encoding_with_show_raw_parses() {
    assert!(parse(&["type", "type-encoding", "List", "test.tg", "--show-raw"]).is_ok());
}

#[test]
fn test_info_type_constructors_parses() {
    assert!(parse(&["type", "constructors", "AB", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_mutual_recursion_groups_parses() {
    assert!(parse(&["type", "mutual-recursion-groups", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_field_type_parses() {
    assert!(parse(&["type", "field-type", "Foo.bar", "test.tg"]).is_ok());
}

// ── Legacy paths (hidden aliases) ──

#[test]
fn test_info_types_legacy_parses() {
    assert!(parse(&["types", "test.tg"]).is_ok());
}

#[test]
fn test_info_adt_legacy_parses() {
    assert!(parse(&["adt", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_adt_legacy_with_flags_parses() {
    assert!(parse(&["adt", "List", "test.tg", "--show-fields"]).is_ok());
}

#[test]
fn test_info_encoding_legacy_parses() {
    assert!(parse(&["encoding", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_type_encoding_legacy_parses() {
    assert!(parse(&["type-encoding", "List", "test.tg"]).is_ok());
}

#[test]
fn test_info_constructors_legacy_parses() {
    assert!(parse(&["constructors", "AB", "test.tg"]).is_ok());
}

#[test]
fn test_info_mutual_recursion_groups_legacy_parses() {
    assert!(parse(&["mutual-recursion-groups", "test.tg"]).is_ok());
}

#[test]
fn test_info_field_type_legacy_parses() {
    assert!(parse(&["field-type", "Foo.bar", "test.tg"]).is_ok());
}

// ── Top-level (unchanged) ──

#[test]
fn test_info_def_parses() {
    assert!(parse(&["def", "main", "test.tg"]).is_ok());
}

#[test]
fn test_info_pipeline_parses() {
    assert!(parse(&["pipeline"]).is_ok());
}

#[test]
fn test_info_module_tree_parses() {
    assert!(parse(&["module", "tree", "test.tg"]).is_ok());
}

#[test]
fn test_info_module_imports_parses() {
    assert!(parse(&["module", "imports", "core", "test.tg"]).is_ok());
}
