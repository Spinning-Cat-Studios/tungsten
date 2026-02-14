//! Tests for module resolution and tree building.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::ast::Visibility;
use crate::driver::PipelineError;

use super::info::build_module_info;
use super::parse::{flatten_module_tree, parse_module_tree, resolve_module_path};
use crate::elaborate::ModulePath;

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

// ─────────────────────────────────────────────────────────────────────────────
// Type Parameter Count Tests (ADR 30.1.26.1)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_type_param_count_for_generic_type() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with a generic type
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "type List<T> = | Nil | Cons(T, List<T>)").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let root = ModulePath::root();
    let contents = info.modules.get(&root).unwrap();

    assert!(contents.types.contains(&"List".to_string()));
    assert_eq!(contents.type_param_counts.get("List"), Some(&1));
}

#[test]
fn test_type_param_count_for_non_generic_type() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with a non-generic type
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "type Point = { x: Int, y: Int }").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let root = ModulePath::root();
    let contents = info.modules.get(&root).unwrap();

    assert!(contents.types.contains(&"Point".to_string()));
    assert_eq!(contents.type_param_counts.get("Point"), Some(&0));
}

#[test]
fn test_type_param_count_for_type_alias() {
    let dir = TempDir::new().unwrap();

    // Create main.tg with a generic type alias
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "type Pair<A, B> = (A, B)").unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let root = ModulePath::root();
    let contents = info.modules.get(&root).unwrap();

    assert!(contents.types.contains(&"Pair".to_string()));
    assert_eq!(contents.type_param_counts.get("Pair"), Some(&2));
}

#[test]
fn test_type_param_count_from_submodule() {
    let dir = TempDir::new().unwrap();

    // Create main.tg that imports from foo
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;").unwrap();

    // Create foo.tg with a generic type
    fs::write(
        dir.path().join("foo.tg"),
        "pub type Maybe<T> = | Nothing | Just(T)",
    )
    .unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let foo_path = ModulePath::from_segments(&["foo".to_string()]);
    let contents = info.modules.get(&foo_path).unwrap();

    assert!(contents.types.contains(&"Maybe".to_string()));
    assert_eq!(contents.type_param_counts.get("Maybe"), Some(&1));
}

#[test]
fn test_type_param_count_propagates_through_pub_use() {
    let dir = TempDir::new().unwrap();

    // Create main.tg that re-exports from foo
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;\npub use foo::Container;").unwrap();

    // Create foo.tg with a generic type
    fs::write(
        dir.path().join("foo.tg"),
        "pub type Container<T, U> = { first: T, second: U }",
    )
    .unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    // Check the source module has the type with correct param count
    let foo_path = ModulePath::from_segments(&["foo".to_string()]);
    let foo_contents = info.modules.get(&foo_path).unwrap();
    assert_eq!(foo_contents.type_param_counts.get("Container"), Some(&2));

    // Check that pub use re-export copies the param count to root module
    let root = ModulePath::root();
    let root_contents = info.modules.get(&root).unwrap();
    assert!(root_contents.types.contains(&"Container".to_string()));
    assert_eq!(root_contents.type_param_counts.get("Container"), Some(&2));
}

#[test]
fn test_type_param_count_propagates_through_glob_reexport() {
    let dir = TempDir::new().unwrap();

    // Create main.tg that glob re-exports from foo
    let main_path = dir.path().join("main.tg");
    fs::write(&main_path, "mod foo;\npub use foo::*;").unwrap();

    // Create foo.tg with generic types
    fs::write(
        dir.path().join("foo.tg"),
        r#"
pub type List<T> = | Nil | Cons(T, List<T>)
pub type Map<K, V> = { key: K, value: V }
"#,
    )
    .unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    // Check that glob re-export copies param counts
    let root = ModulePath::root();
    let root_contents = info.modules.get(&root).unwrap();

    assert!(root_contents.types.contains(&"List".to_string()));
    assert_eq!(root_contents.type_param_counts.get("List"), Some(&1));

    assert!(root_contents.types.contains(&"Map".to_string()));
    assert_eq!(root_contents.type_param_counts.get("Map"), Some(&2));
}
/// Test that module info captures re-export structure (ADR 31 infrastructure).
///
/// This tests the infrastructure for cross-module generic type resolution:
/// - core::option defines Option<T>
/// - parser::option re-exports Option  
///
/// Currently, process_pub_use_reexports copies items to the re-exporting module's
/// types list. The canonical lookup (ADR 31) will use defining_module in TypeDef
/// and item_modules to trace back to the original definition.
#[test]
fn test_canonical_type_resolution_through_reexport() {
    let dir = TempDir::new().unwrap();

    // Create directory structure
    let core_dir = dir.path().join("core");
    fs::create_dir(&core_dir).unwrap();
    let parser_dir = dir.path().join("parser");
    fs::create_dir(&parser_dir).unwrap();

    // main.tg
    let main_path = dir.path().join("main.tg");
    fs::write(
        &main_path,
        r#"
pub mod core;
pub mod parser;

// Import Option through parser (re-export chain)
use parser::option::Option;

fn test() -> Option<Nat> {
    None
}
"#,
    )
    .unwrap();

    // core/mod.tg
    fs::write(core_dir.join("mod.tg"), "pub mod option;").unwrap();

    // core/option.tg - Original definition
    fs::write(
        core_dir.join("option.tg"),
        r#"
pub type Option<T> = | None | Some(T)
"#,
    )
    .unwrap();

    // parser/mod.tg
    fs::write(parser_dir.join("mod.tg"), "pub mod option;").unwrap();

    // parser/option.tg - Re-exports from core
    fs::write(
        parser_dir.join("option.tg"),
        r#"
pub use core::option::Option;
pub use core::option::None;
pub use core::option::Some;
"#,
    )
    .unwrap();

    // Parse and build module info
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    // Verify the module structure exists
    let parser_option = ModulePath::new(vec!["parser".to_string(), "option".to_string()]);
    let core_option = ModulePath::new(vec!["core".to_string(), "option".to_string()]);

    assert!(
        info.modules.contains_key(&parser_option),
        "parser::option module should exist"
    );
    assert!(
        info.modules.contains_key(&core_option),
        "core::option module should exist"
    );

    // Verify core::option has Option as a defined type with correct param count
    let core_option_contents = info.modules.get(&core_option).unwrap();
    assert!(
        core_option_contents.types.contains(&"Option".to_string()),
        "core::option should define Option type"
    );
    assert_eq!(
        core_option_contents.type_param_counts.get("Option"),
        Some(&1),
        "Option should have 1 type parameter"
    );

    // Verify core::option has None and Some constructors
    assert!(
        core_option_contents
            .constructors
            .contains(&"None".to_string()),
        "core::option should have None constructor"
    );
    assert!(
        core_option_contents
            .constructors
            .contains(&"Some".to_string()),
        "core::option should have Some constructor"
    );

    // Verify item_modules tracks the ORIGINAL defining module for Option
    // This is key for canonical lookup - even though parser::option re-exports Option,
    // item_modules should point to core::option as the original definer
    assert_eq!(
        info.item_modules.get("Option"),
        Some(&core_option),
        "item_modules should track Option's original defining module (core::option)"
    );

    // Current behavior: re-exports copy to parser::option's types list
    // This is what canonical lookup needs to handle - parser::option.types
    // contains "Option" but it's not the canonical definition
    let parser_option_contents = info.modules.get(&parser_option).unwrap();
    assert!(
        parser_option_contents.types.contains(&"Option".to_string()),
        "parser::option should have Option in types list (via re-export)"
    );
    assert_eq!(
        parser_option_contents.type_param_counts.get("Option"),
        Some(&1),
        "Re-exported Option should preserve param count"
    );
}
