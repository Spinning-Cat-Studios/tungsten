//! Tests for pub use re-export processing, specifically post-order traversal.
//!
//! Regression test for ADR 8.5.26a: nested directory module re-exports
//! require post-order processing so that chained `pub use` declarations
//! see their children's re-exports.

use std::collections::HashSet;
use tempfile::TempDir;

use crate::driver::modules::info::build_module_info;
use crate::driver::modules::parse::parse_module_tree;
use crate::elaborate::ModulePath;

/// Regression test: chained pub use through nested directory modules.
///
/// Structure:
///   main.tg:          mod parent;
///   parent/mod.tg:    mod child; pub use parent::child::*;
///   parent/child.tg:  pub type Foo = { x: Nat }  pub fn bar() -> Nat { 42 }
///
/// Without post-order traversal, `parent`'s `pub use parent::child::*`
/// runs BEFORE `child`'s items are registered, so `Foo` never appears
/// in `parent`'s module contents.
#[test]
fn test_chained_pub_use_reexport_postorder() {
    let dir = TempDir::new().unwrap();

    std::fs::write(dir.path().join("main.tg"), "mod parent;\n").unwrap();

    let parent_dir = dir.path().join("parent");
    std::fs::create_dir(&parent_dir).unwrap();
    std::fs::write(
        parent_dir.join("mod.tg"),
        "mod child;\npub use parent::child::*;\n",
    )
    .unwrap();

    std::fs::write(
        parent_dir.join("child.tg"),
        "pub type Foo = { x: Nat }\npub fn bar() -> Nat { 42 }\n",
    )
    .unwrap();

    let main_path = dir.path().join("main.tg");
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let parent_path = ModulePath::root().child("parent".to_string());
    let parent_contents = info
        .modules
        .get(&parent_path)
        .expect("parent module should exist");

    assert!(
        parent_contents.types.iter().any(|n| n == "Foo"),
        "Foo should be re-exported into parent via pub use parent::child::*"
    );
    assert!(
        parent_contents.values.iter().any(|n| n == "bar"),
        "bar should be re-exported into parent via pub use parent::child::*"
    );
}

/// Two levels of chained re-exports: grandchild → child → parent.
#[test]
fn test_double_chained_pub_use_reexport() {
    let dir = TempDir::new().unwrap();

    std::fs::write(dir.path().join("main.tg"), "mod top;\n").unwrap();

    let top_dir = dir.path().join("top");
    std::fs::create_dir(&top_dir).unwrap();
    std::fs::write(top_dir.join("mod.tg"), "mod mid;\npub use top::mid::*;\n").unwrap();

    let mid_dir = top_dir.join("mid");
    std::fs::create_dir(&mid_dir).unwrap();
    std::fs::write(
        mid_dir.join("mod.tg"),
        "mod leaf;\npub use top::mid::leaf::*;\n",
    )
    .unwrap();

    std::fs::write(mid_dir.join("leaf.tg"), "pub type Quux = { val: Nat }\n").unwrap();

    let main_path = dir.path().join("main.tg");
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);

    let mid_path = ModulePath::root()
        .child("top".to_string())
        .child("mid".to_string());
    let mid_contents = info.modules.get(&mid_path).unwrap();
    assert!(
        mid_contents.types.iter().any(|n| n == "Quux"),
        "Quux should be re-exported into mid"
    );

    let top_path = ModulePath::root().child("top".to_string());
    let top_contents = info.modules.get(&top_path).unwrap();
    assert!(
        top_contents.types.iter().any(|n| n == "Quux"),
        "Quux should be re-exported from mid into top"
    );
}
