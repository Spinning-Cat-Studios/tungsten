//! Extraction and the command, over real `.tg` trees — the only tier that can
//! catch the extraction disagreeing with the parser.

use std::path::Path;
use std::process::ExitCode;

use super::*;

#[test]
fn discovery_finds_the_sibling_test_entries_and_not_the_driver() {
    let dir = fixture_project();
    let found = discover_test_entries(&dir.path().join("main.tg"));
    let names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["test_emitter.tg".to_string()]);
}

#[test]
fn extraction_keys_modules_below_the_root_so_two_entry_files_agree() {
    let dir = fixture_project();
    let from_driver = module_graph(&dir.path().join("main.tg")).expect("driver graph");
    let from_test = module_graph(&dir.path().join("test_emitter.tg")).expect("test graph");

    assert_eq!(
        from_driver.modules,
        set(&["engine", "engine::core", "engine::emitter"])
    );
    assert_eq!(from_driver.modules, from_test.modules);
    assert_eq!(from_driver.entry_uses, set(&["engine::core"]));
    assert_eq!(from_test.entry_uses, set(&["engine::emitter"]));
}

#[test]
fn a_re_exporting_parent_does_not_import_itself() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::create_dir(root.join("engine")).expect("mkdir");
    fs::write(
        root.join("engine/mod.tg"),
        "pub mod core;\npub use engine::core::core_value;\n",
    )
    .expect("write");
    fs::write(
        root.join("engine/core.tg"),
        "pub fn core_value() -> Nat { 1 }\n",
    )
    .expect("write");
    fs::write(
        root.join("main.tg"),
        "mod engine;\npub fn main() -> Nat { 0 }\n",
    )
    .expect("write");

    let graph = module_graph(&root.join("main.tg")).expect("graph");
    assert_eq!(graph.uses.get("engine"), Some(&set(&["engine::core"])));
}

/// A glob and an alias are edges too. `codegen/mod.tg` re-exports its whole
/// subsystem with `pub use codegen::ir_builder::*`, so dropping the glob arm
/// would silently disconnect every re-exporting parent from its children.
#[test]
fn a_glob_import_and_an_aliased_import_are_both_edges() {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    fs::create_dir(root.join("engine")).expect("mkdir");
    fs::write(
        root.join("engine/mod.tg"),
        "pub mod core;\npub mod emitter;\n",
    )
    .expect("write");
    fs::write(
        root.join("engine/core.tg"),
        "pub fn core_value() -> Nat { 1 }\n",
    )
    .expect("write");
    fs::write(
        root.join("engine/emitter.tg"),
        "use engine::core::*;\npub fn emit() -> Nat { core_value() }\n",
    )
    .expect("write");
    fs::write(
        root.join("main.tg"),
        "mod engine;\nuse engine::emitter::emit as run;\npub fn main() -> Nat { run() }\n",
    )
    .expect("write");

    let graph = module_graph(&root.join("main.tg")).expect("graph");
    assert_eq!(graph.entry_uses, set(&["engine::emitter"]), "alias edge");
    assert_eq!(
        graph.uses.get("engine::emitter"),
        Some(&set(&["engine::core"])),
        "glob edge"
    );
}

#[test]
fn a_directory_with_no_sibling_entry_files_discovers_none() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("main.tg"), "pub fn main() -> Nat { 0 }\n").expect("write");
    assert!(discover_test_entries(&dir.path().join("main.tg")).is_empty());
}

#[test]
fn discovery_over_an_unreadable_directory_yields_none_rather_than_failing() {
    // The report says "no test entry file was read" on its own line, which is
    // more useful than an exit code; a bare filename has no parent directory.
    assert!(discover_test_entries(Path::new("main.tg")).is_empty());
    assert!(discover_test_entries(Path::new("/no/such/dir/main.tg")).is_empty());
}

#[test]
fn the_command_partitions_a_real_project_and_succeeds() {
    // Reporting, never gating: a finding must not fail the build.
    let dir = fixture_project();
    assert_eq!(
        cmd_audit_driver_reach(&dir.path().join("main.tg"), &[]),
        ExitCode::SUCCESS
    );
}

#[test]
fn the_command_accepts_an_extra_test_entry() {
    let dir = fixture_project();
    let extra = vec![dir.path().join("test_emitter.tg")];
    assert_eq!(
        cmd_audit_driver_reach(&dir.path().join("main.tg"), &extra),
        ExitCode::SUCCESS
    );
}

#[test]
fn an_unparseable_driver_entry_fails_rather_than_reporting_an_empty_partition() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("main.tg");
    fs::write(&path, "mod does_not_exist;\n").expect("write");
    assert_eq!(cmd_audit_driver_reach(&path, &[]), ExitCode::FAILURE);
}

#[test]
fn a_missing_driver_entry_fails() {
    let dir = TempDir::new().expect("tempdir");
    assert_eq!(
        cmd_audit_driver_reach(&dir.path().join("nope.tg"), &[]),
        ExitCode::FAILURE
    );
}

/// The driver entry must never also root the test side: it would seed
/// `test_reached` with the driver's own imports and empty `test_only`.
#[test]
fn the_driver_entry_is_excluded_from_its_own_test_roots() {
    let dir = fixture_project();
    let driver = dir.path().join("main.tg");
    let explicit = vec![driver.clone(), dir.path().join("test_emitter.tg")];

    let entries = test_entries_for(&driver, &explicit);
    assert!(!entries.contains(&driver), "{entries:?}");
    assert_eq!(entries, vec![dir.path().join("test_emitter.tg")]);
}

/// The same exclusion when the driver entry's *own* stem is `test_*`, which is
/// what running the census on a suite looks like.
#[test]
fn a_test_prefixed_driver_entry_is_excluded_from_the_roots_discovery_found() {
    let dir = fixture_project();
    let driver = dir.path().join("test_emitter.tg");
    assert!(discover_test_entries(&driver).contains(&driver));
    assert!(test_entries_for(&driver, &[]).is_empty());
}

#[test]
fn an_explicit_entry_already_discovered_is_not_counted_twice() {
    let dir = fixture_project();
    let discovered = dir.path().join("test_emitter.tg");
    let entries = test_entries_for(&dir.path().join("main.tg"), &[discovered.clone()]);
    assert_eq!(entries, vec![discovered]);
}

/// `absorb` merges each entry file's graph into one shared adjacency; without
/// it every partition is empty and only the exit code says otherwise.
#[test]
fn every_entry_files_modules_and_edges_land_in_one_shared_input() {
    let dir = fixture_project();
    let driver = dir.path().join("main.tg");
    let input = collect_input(&driver, &test_entries_for(&driver, &[])).expect("driver parsed");

    assert_eq!(
        input.modules,
        set(&["engine", "engine::core", "engine::emitter"])
    );
    assert_eq!(
        input.uses.get(&driver.display().to_string()),
        Some(&set(&["engine::core"])),
        "the driver entry's own imports"
    );
    assert_eq!(
        input
            .uses
            .get(&dir.path().join("test_emitter.tg").display().to_string()),
        Some(&set(&["engine::emitter"])),
        "the test entry's imports, keyed separately"
    );

    let reach = partition_reach(&input);
    assert_eq!(reach.driver_reached, set(&["engine", "engine::core"]));
    assert_eq!(reach.test_only, set(&["engine::emitter"]));
}

#[test]
fn an_unparseable_test_entry_is_named_rather_than_failing_the_run() {
    let dir = fixture_project();
    fs::write(dir.path().join("test_broken.tg"), "mod nowhere;\n").expect("write");
    let input = collect_input(
        &dir.path().join("main.tg"),
        &discover_test_entries(&dir.path().join("main.tg")),
    )
    .expect("driver parsed");

    assert_eq!(input.unreadable_entries.len(), 1);
    assert!(
        input
            .unreadable_entries
            .iter()
            .any(|e| e.ends_with("test_broken.tg")),
        "{:?}",
        input.unreadable_entries
    );
}
