use super::*;
use tempfile::TempDir;

/// Helper: parse a module tree from a temp directory and check reexports.
fn check_dir(dir: &TempDir) -> Vec<ReexportIssue> {
    let main_path = dir.path().join("main.tg");
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);
    check_reexports(&tree, &ModulePath::root(), &info)
}

/// Broken glob: source module doesn't exist → UnresolvedModule.
#[test]
fn test_empty_glob_detected() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("main.tg"), "pub use nonexistent::*;\n").unwrap();

    let issues = check_dir(&dir);
    assert!(
        !issues.is_empty(),
        "should detect unresolved glob re-export"
    );
    assert!(
        matches!(&issues[0], ReexportIssue::UnresolvedModule { .. }),
        "should be UnresolvedModule, got: {:?}",
        issues[0]
    );
}

/// Broken named re-export: source module exists but item name is wrong.
#[test]
fn test_missing_named_item_detected() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod child;\npub use child::{DoesNotExist};\n",
    )
    .unwrap();

    let child_dir = dir.path().join("child");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(child_dir.join("mod.tg"), "pub type RealType = { x: Nat }\n").unwrap();

    let issues = check_dir(&dir);
    assert!(!issues.is_empty(), "should detect missing named item");
    assert!(
        matches!(&issues[0], ReexportIssue::MissingNamedItem { item_name, .. } if item_name == "DoesNotExist"),
        "should be MissingNamedItem for DoesNotExist, got: {:?}",
        issues[0]
    );
}

/// Successful re-export: no issues reported for a valid pub use.
#[test]
fn test_valid_reexport_no_issues() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod child;\npub use child::{Foo};\n",
    )
    .unwrap();

    let child_dir = dir.path().join("child");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(child_dir.join("mod.tg"), "pub type Foo = { x: Nat }\n").unwrap();

    let issues = check_dir(&dir);
    assert!(
        issues.is_empty(),
        "valid re-export should produce no issues, got: {:?}",
        issues
    );
}

/// Source module exists but has zero public items → no issue (nothing to copy).
#[test]
fn test_empty_source_module_glob() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod child;\npub use child::*;\n",
    )
    .unwrap();

    let child_dir = dir.path().join("child");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(child_dir.join("mod.tg"), "// empty module\n").unwrap();

    let issues = check_dir(&dir);
    assert!(
        issues.is_empty(),
        "glob from empty source should not be flagged, got: {:?}",
        issues
    );
}

/// `pub(crate) use` visibility is also checked (not just `pub use`).
#[test]
fn test_crate_visibility_reexport() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod child;\npub(crate) use child::{DoesNotExist};\n",
    )
    .unwrap();

    let child_dir = dir.path().join("child");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(child_dir.join("mod.tg"), "pub type RealType = { x: Nat }\n").unwrap();

    let issues = check_dir(&dir);
    assert!(
        !issues.is_empty(),
        "crate use with missing item should be detected"
    );
    assert!(
        matches!(&issues[0], ReexportIssue::MissingNamedItem { item_name, .. } if item_name == "DoesNotExist"),
        "should be MissingNamedItem, got: {:?}",
        issues[0]
    );
}

/// Named item exists in source but was not copied into target → NotCopied.
#[test]
fn test_not_copied_named_item() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod child;\npub use child::{Foo};\n",
    )
    .unwrap();

    let child_dir = dir.path().join("child");
    std::fs::create_dir(&child_dir).unwrap();
    std::fs::write(child_dir.join("mod.tg"), "pub type Foo = { x: Nat }\n").unwrap();

    let main_path = dir.path().join("main.tg");
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    // Build module info then manually remove Foo from root to simulate copy failure.
    let mut info = build_module_info(&tree);
    let root = ModulePath::root();
    if let Some(root_contents) = info.modules.get_mut(&root) {
        root_contents.types.retain(|n| n != "Foo");
    }

    let issues = check_reexports(&tree, &root, &info);
    assert!(!issues.is_empty(), "should detect not-copied item");
    assert!(
        matches!(&issues[0], ReexportIssue::NotCopied { item_name, .. } if item_name == "Foo"),
        "should be NotCopied for Foo, got: {:?}",
        issues[0]
    );
}
