//! Tests for module visibility in parsed module trees.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::ast::Visibility;

use crate::driver::modules::parse::parse_module_tree;
#[test]
fn test_root_module_visibility_is_public() {
    let dir = TempDir::new().unwrap();
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "fn hello() -> Nat { 42 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    // Root module is always public (crate entry point)
    assert_eq!(tree.visibility, Visibility::Public);
}

#[test]
fn test_submodule_visibility_private() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with private mod internal;
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod internal;").unwrap();

    // Create internal.tg
    fs::write(dir.path().join("internal.tg"), "fn secret() -> Nat { 1 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.visibility, Visibility::Public);
    assert_eq!(tree.submodules.len(), 1);
    assert_eq!(tree.submodules[0].visibility, Visibility::Private);
}

#[test]
fn test_submodule_visibility_public() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with pub mod api;
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "pub mod api;").unwrap();

    // Create api.tg
    fs::write(dir.path().join("api.tg"), "fn public_fn() -> Nat { 1 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.visibility, Visibility::Public);
    assert_eq!(tree.submodules.len(), 1);
    assert_eq!(tree.submodules[0].visibility, Visibility::Public);
}

#[test]
fn test_mixed_visibility_in_module_tree() {
    let dir = TempDir::new().unwrap();

    // main.tg with both pub and private modules
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "pub mod api;\nmod internal;\npub mod types;").unwrap();

    // Create submodule files
    fs::write(dir.path().join("api.tg"), "fn a() -> Nat { 1 }").unwrap();
    fs::write(dir.path().join("internal.tg"), "fn b() -> Nat { 2 }").unwrap();
    fs::write(dir.path().join("types.tg"), "fn c() -> Nat { 3 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.submodules.len(), 3);
    assert_eq!(tree.submodules[0].visibility, Visibility::Public); // api
    assert_eq!(tree.submodules[1].visibility, Visibility::Private); // internal
    assert_eq!(tree.submodules[2].visibility, Visibility::Public); // types
}

#[test]
fn test_nested_visibility_propagation() {
    let dir = TempDir::new().unwrap();

    // main.tg -> pub mod api; -> api/mod.tg -> mod helpers;
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "pub mod api;").unwrap();

    let api_dir = dir.path().join("api");
    fs::create_dir(&api_dir).unwrap();
    fs::write(api_dir.join("mod.tg"), "mod helpers;").unwrap();
    fs::write(api_dir.join("helpers.tg"), "fn h() -> Nat { 1 }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    assert_eq!(tree.visibility, Visibility::Public); // main
    assert_eq!(tree.submodules[0].visibility, Visibility::Public); // api (pub mod)
    assert_eq!(
        tree.submodules[0].submodules[0].visibility,
        Visibility::Private
    ); // helpers (mod)
}
