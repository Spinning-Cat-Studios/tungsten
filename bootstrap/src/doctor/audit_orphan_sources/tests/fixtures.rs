//! The walk, the subtraction, the build-file scan and the command over real
//! trees — the only tier that can catch the extraction disagreeing with the
//! parser (AC 1).

use std::process::ExitCode;

use super::*;

#[test]
fn the_walk_finds_every_tg_file_below_the_root_and_nothing_else() {
    let dir = fixture_project();
    assert_eq!(
        walk_tg_files(&dir.path().join("project")),
        set(&[
            "engine/core.tg",
            "engine/core_dev.tg",
            "engine/mod.tg",
            "engine/spare.tg",
            "main.tg",
            "test_core.tg",
        ]),
        "the Makefile is not a .tg file and must not be walked"
    );
}

#[test]
fn the_walk_skips_hidden_directories_such_as_the_elaboration_cache() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::create_dir(root.join(".tungsten")).expect("mkdir");
    fs::write(root.join(".tungsten/cached.tg"), "").expect("write");
    fs::write(root.join("main.tg"), "pub fn main() -> Nat { 0 }\n").expect("write");
    assert_eq!(walk_tg_files(root), set(&["main.tg"]));
}

#[test]
fn a_walk_of_a_missing_directory_finds_nothing_rather_than_failing() {
    assert!(walk_tg_files(Path::new("/no/such/directory")).is_empty());
}

/// The subtraction is the parser's answer, so the extraction cannot drift from
/// what the compiler actually compiles.
#[test]
fn the_subtraction_is_the_parsed_tree_and_excludes_the_entry_file_itself() {
    let dir = fixture_project();
    let project = dir.path().join("project");
    let declared = declared_module_files(&project.join("main.tg"), &project).expect("parsed");
    assert_eq!(declared, set(&["engine/mod.tg", "engine/core.tg"]));
}

#[test]
fn the_census_over_a_real_tree_separates_stranded_from_build_swapped() {
    let dir = fixture_project();
    let project = dir.path().join("project");
    let collected = collect_input(&project.join("main.tg")).expect("driver parsed");
    let sources = classify_orphans(&collected);

    assert_eq!(sources.stranded, set(&["engine/spare.tg"]));
    assert_eq!(sources.build_swapped, set(&["engine/core_dev.tg"]));
    assert_eq!(sources.entry_roots, set(&["main.tg", "test_core.tg"]));
    assert_eq!(sources.files_walked, 6);
    assert_eq!(
        sources.entry_files_read, 2,
        "the driver and its test sibling"
    );
    assert!(
        sources.build_files_scanned >= 1,
        "the fixture Makefile must be read, else build-swapped is vacuous"
    );
}

#[test]
fn the_repo_root_is_the_nearest_ancestor_holding_a_makefile() {
    let dir = fixture_project();
    assert_eq!(
        repo_root_of(&dir.path().join("project/engine")).as_deref(),
        Some(dir.path())
    );
}

#[test]
fn a_tree_with_no_makefile_above_it_has_no_repo_root() {
    let dir = TempDir::new().expect("tempdir");
    // A `TempDir` under /tmp or /var — neither has a Makefile at any ancestor.
    assert!(repo_root_of(dir.path()).is_none());
}

#[test]
fn build_file_texts_reads_the_makefile_at_the_repo_root() {
    let dir = fixture_project();
    let texts = build_file_texts(dir.path());
    assert!(texts.iter().any(|t| t.contains("core_dev.tg")), "{texts:?}");
}

/// The scan descends `make/` and skips build output. A `.mk` under `target/` is
/// a generated copy of a recipe, and honouring it would classify a file as
/// build-swapped on the strength of an artefact nobody wrote.
#[test]
fn the_build_scan_descends_scan_roots_and_skips_build_output() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::write(root.join("Makefile"), "include make/native.mk\n").expect("write");
    fs::create_dir(root.join("make")).expect("mkdir");
    fs::write(root.join("make/native.mk"), "\t@cp a/live.tg a/real.tg\n").expect("write");
    fs::create_dir(root.join("make/target")).expect("mkdir");
    fs::write(
        root.join("make/target/stale.mk"),
        "\t@cp a/stale.tg a/x.tg\n",
    )
    .expect("write");
    fs::write(root.join("make/notes.md"), "a/prose.tg\n").expect("write");

    let texts = build_file_texts(root);
    let candidates = set(&["a/live.tg", "a/stale.tg", "a/prose.tg"]);
    assert_eq!(
        mentioned_in_build_files(&candidates, &texts),
        set(&["a/live.tg"]),
        "a recipe under target/ and a markdown note are not build files"
    );
}

/// An entry file named without a directory walks `.`, not the empty path —
/// which reads nothing and renders as a clean census rather than an unrun one.
#[test]
fn a_bare_entry_filename_walks_the_current_directory() {
    assert_eq!(walk_root_of(Path::new("main.tg")), Some(Path::new(".")));
    assert_eq!(
        walk_root_of(Path::new("src/compiler/main.tg")),
        Some(Path::new("src/compiler"))
    );
    assert_eq!(walk_root_of(Path::new("/")), None);
}

#[test]
fn the_command_censuses_a_real_project_and_succeeds() {
    // Reporting, never gating (D3): a finding must not fail the build.
    let dir = fixture_project();
    assert_eq!(
        cmd_audit_orphan_sources(&dir.path().join("project/main.tg")),
        ExitCode::SUCCESS
    );
}

#[test]
fn an_unparseable_entry_file_fails_rather_than_reporting_everything_stranded() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("main.tg");
    fs::write(&path, "mod does_not_exist;\n").expect("write");
    assert_eq!(cmd_audit_orphan_sources(&path), ExitCode::FAILURE);
}

#[test]
fn a_missing_entry_file_fails() {
    let dir = TempDir::new().expect("tempdir");
    assert_eq!(
        cmd_audit_orphan_sources(&dir.path().join("nope.tg")),
        ExitCode::FAILURE
    );
}

/// A sibling entry file that does not parse is named, not silently skipped:
/// its modules go unsubtracted, so `stranded` is inflated by exactly them.
#[test]
fn an_unparseable_sibling_entry_is_named_rather_than_failing_the_run() {
    let dir = fixture_project();
    let project = dir.path().join("project");
    fs::write(project.join("test_broken.tg"), "mod nowhere;\n").expect("write");

    let collected = collect_input(&project.join("main.tg")).expect("driver parsed");
    assert_eq!(collected.unreadable_entries, set(&["test_broken.tg"]));
    assert!(!collected.entry_files.contains("test_broken.tg"));
}
