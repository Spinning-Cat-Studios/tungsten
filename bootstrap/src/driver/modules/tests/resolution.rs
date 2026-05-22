//! Tests for module path resolution, tree parsing, and flattening.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::ast::Visibility;
use crate::driver::PipelineError;

use crate::driver::modules::parse::{flatten_module_tree, parse_module_tree, resolve_module_path};
#[test]
fn test_resolve_module_file() {
    let dir = TempDir::new().unwrap();
    let foo_path = dir.path().join("foo.tg");
    fs::write(&foo_path, "fn x() -> Nat { 1 }").unwrap();

    let result = resolve_module_path(dir.path(), "foo", dir.path().join("mod.tg").as_path());
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), foo_path);
}

#[test]
fn test_resolve_module_dir() {
    let dir = TempDir::new().unwrap();
    let foo_dir = dir.path().join("foo");
    fs::create_dir(&foo_dir).unwrap();
    let mod_path = foo_dir.join("mod.tg");
    fs::write(&mod_path, "fn y() -> Nat { 2 }").unwrap();

    let result = resolve_module_path(dir.path(), "foo", dir.path().join("mod.tg").as_path());
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), mod_path);
}

#[test]
fn test_resolve_module_ambiguous() {
    let dir = TempDir::new().unwrap();

    // Create both foo.tg and foo/mod.tg
    let foo_file = dir.path().join("foo.tg");
    fs::write(&foo_file, "fn x() -> Nat { 1 }").unwrap();

    let foo_dir = dir.path().join("foo");
    fs::create_dir(&foo_dir).unwrap();
    let foo_mod = foo_dir.join("mod.tg");
    fs::write(&foo_mod, "fn y() -> Nat { 2 }").unwrap();

    let result = resolve_module_path(dir.path(), "foo", dir.path().join("mod.tg").as_path());
    assert!(matches!(result, Err(PipelineError::AmbiguousModule { .. })));
}

#[test]
fn test_resolve_module_not_found() {
    let dir = TempDir::new().unwrap();

    let result = resolve_module_path(dir.path(), "bar", dir.path().join("mod.tg").as_path());
    assert!(matches!(result, Err(PipelineError::ModuleNotFound { .. })));
}

#[test]
fn test_parse_module_tree_single_file() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "fn hello() -> Nat { 42 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.path, main_path);
    assert_eq!(tree.source_file.items.len(), 1);
    assert!(tree.submodules.is_empty());
}

#[test]
fn test_parse_module_tree_with_submodule() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with mod foo;
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;\nfn main() -> Nat { x() }").unwrap();

    // Create foo.tg
    let foo_path = dir.path().join("foo.tg");
    fs::write(&foo_path, "fn x() -> Nat { 1 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.submodules.len(), 1);
    assert_eq!(tree.submodules[0].path, foo_path);
}

#[test]
fn test_parse_module_tree_nested() {
    let dir = TempDir::new().unwrap();

    // main.tg -> mod foo; -> foo/mod.tg -> mod bar; -> foo/bar.tg
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;").unwrap();

    let foo_dir = dir.path().join("foo");
    fs::create_dir(&foo_dir).unwrap();
    fs::write(foo_dir.join("mod.tg"), "mod bar;").unwrap();
    fs::write(foo_dir.join("bar.tg"), "fn nested() -> Nat { 3 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.submodules.len(), 1); // foo
    assert_eq!(tree.submodules[0].submodules.len(), 1); // foo.bar
}

#[test]
fn test_flatten_module_tree() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with a function and mod foo;
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;\nfn main_fn() -> Nat { 1 }").unwrap();

    // Create foo.tg with a function
    fs::write(dir.path().join("foo.tg"), "fn foo_fn() -> Nat { 2 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    let items = flatten_module_tree(&tree);

    // Should have 2 items: main_fn and foo_fn (mod foo is excluded)
    assert_eq!(items.len(), 2);
}
