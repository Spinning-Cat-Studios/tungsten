//! Tests for type parameter count tracking and propagation through re-exports.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::driver::modules::info::build_module_info;
use crate::driver::modules::parse::parse_module_tree;
use crate::elaborate::ModulePath;

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

/// Build a reexport fixture with core::option defining Option<T> and
/// parser::option re-exporting it. Returns (TempDir, ModuleInfo).
fn build_reexport_fixture() -> (TempDir, super::super::info::ModuleInfo) {
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
        "pub type Option<T> = | None | Some(T)\n",
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

    (dir, info)
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
    let (_dir, info) = build_reexport_fixture();

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
    assert_eq!(
        info.item_modules.get("Option"),
        Some(&core_option),
        "item_modules should track Option's original defining module (core::option)"
    );

    // Current behavior: re-exports copy to parser::option's types list
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
