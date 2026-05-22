use super::*;
use std::fs;
use tempfile::TempDir;

/// Create a temp dir with `.tg` files for testing.
fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.tg"), "fn x() -> Nat { 0 }").unwrap();
    fs::write(dir.path().join("helper.tg"), "fn y() -> Nat { 1 }").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(
        dir.path().join("sub").join("nested.tg"),
        "fn z() -> Nat { 2 }",
    )
    .unwrap();
    // Hidden dir should be skipped
    fs::create_dir(dir.path().join(".hidden")).unwrap();
    fs::write(dir.path().join(".hidden").join("secret.tg"), "").unwrap();
    // target dir should be skipped
    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target").join("build.tg"), "").unwrap();
    // Non-.tg file should be skipped
    fs::write(dir.path().join("readme.md"), "# hello").unwrap();
    dir
}

#[test]
fn discover_tg_files_finds_recursive_and_skips_hidden() {
    let dir = setup_test_dir();
    let mut files = discover_tg_files(dir.path());
    files.sort();
    let names: Vec<&str> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();
    assert!(names.contains(&"main.tg"), "should find main.tg");
    assert!(names.contains(&"helper.tg"), "should find helper.tg");
    assert!(names.contains(&"nested.tg"), "should find sub/nested.tg");
    assert!(
        !names.contains(&"secret.tg"),
        "should skip .hidden/secret.tg"
    );
    assert!(!names.contains(&"build.tg"), "should skip target/build.tg");
    assert_eq!(files.len(), 3);
}

#[test]
fn parse_files_parallel_produces_valid_asts() {
    let dir = setup_test_dir();
    let files = discover_tg_files(dir.path());
    let preparsed = parse_files_parallel(&files);
    assert_eq!(preparsed.len(), 3, "should parse all 3 .tg files");
    for (path, ast) in &preparsed {
        assert!(
            !ast.items.is_empty(),
            "AST for {} should have items",
            path.display(),
        );
    }
}

#[test]
fn preparsed_matches_serial_parse() {
    let dir = setup_test_dir();
    let main_path = dir.path().join("main.tg");

    // Serial parse
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let serial = parse_module_tree(&main_path, &mut visited, &mut chain, None).unwrap();

    // Pre-parsed parse
    let files = discover_tg_files(dir.path());
    let preparsed = parse_files_parallel(&files);
    let mut visited2 = HashSet::new();
    let mut chain2 = Vec::new();
    let parallel =
        parse_module_tree_with_preparsed(&main_path, &mut visited2, &mut chain2, None, &preparsed)
            .unwrap();

    // Compare: same items, same path
    assert_eq!(
        serial.source_file.items.len(),
        parallel.source_file.items.len()
    );
    assert_eq!(serial.path, parallel.path);
    assert_eq!(serial.submodules.len(), parallel.submodules.len());
}
