//! The three-way split and the two predicates it rests on, over injected data
//! only — no filesystem access in anything under test here (AC 2).

use super::*;

/// The three classes over the shape ADR 3.9.26q's §1.1 records, injected
/// rather than walked.
#[test]
fn the_remainder_splits_into_stranded_build_swapped_and_entry_files() {
    let sources = classify_orphans(&input(
        &[
            "main.tg",
            "test_codegen.tg",
            "parser/mod.tg",
            "parser/self_test.tg",
            "parser/tests/samples.tg",
            "driver/ffi/diagnostics.tg",
            "driver/ffi/diagnostics_dev.tg",
        ],
        &["parser/mod.tg", "driver/ffi/diagnostics.tg"],
        &["main.tg", "test_codegen.tg"],
        &["driver/ffi/diagnostics_dev.tg"],
    ));

    assert_eq!(
        sources.stranded,
        set(&["parser/self_test.tg", "parser/tests/samples.tg"])
    );
    assert_eq!(
        sources.build_swapped,
        set(&["driver/ffi/diagnostics_dev.tg"])
    );
    assert_eq!(sources.entry_roots, set(&["main.tg", "test_codegen.tg"]));
}

/// The reach line's counts are the inputs, not the outputs: a census that
/// subtracted nothing and one that found nothing must be distinguishable.
#[test]
fn the_reach_counts_report_what_was_read_not_what_was_found() {
    let sources = classify_orphans(&input(
        &["main.tg", "a.tg", "b.tg"],
        &["a.tg"],
        &["main.tg"],
        &[],
    ));
    assert_eq!(sources.files_walked, 3);
    assert_eq!(sources.entry_files_read, 1);
    assert_eq!(sources.modules_subtracted, 1);
    assert_eq!(sources.build_files_scanned, 1);
}

/// A declared file is never a finding, whatever its name.
#[test]
fn a_declared_file_is_in_no_class() {
    let sources = classify_orphans(&input(&["parser/mod.tg"], &["parser/mod.tg"], &[], &[]));
    assert!(sources.stranded.is_empty());
    assert!(sources.build_swapped.is_empty());
    assert!(sources.entry_roots.is_empty());
}

/// An entry file a recipe also names is still an entry file. `make` recipes
/// spell `src/compiler/main.tg` constantly, and classifying it as a copy-over
/// template would put the driver root in the class D2 invented for one file.
#[test]
fn an_entry_file_named_by_a_recipe_is_not_build_swapped() {
    let sources = classify_orphans(&input(&["main.tg"], &[], &["main.tg"], &["main.tg"]));
    assert_eq!(sources.entry_roots, set(&["main.tg"]));
    assert!(sources.build_swapped.is_empty());
}

/// Entry-file recognition is by naming convention as well as by membership, so
/// a suite the walk never rooted at is still not reported as stranded.
#[test]
fn entry_shaped_stems_are_recognised_without_being_injected() {
    let sources = classify_orphans(&input(
        &["main.tg", "test_thing.tg", "mustfail_thing.tg", "tester.tg"],
        &[],
        &[],
        &[],
    ));
    assert_eq!(
        sources.entry_roots,
        set(&["main.tg", "test_thing.tg", "mustfail_thing.tg"])
    );
    assert_eq!(
        sources.stranded,
        set(&["tester.tg"]),
        "`tester` is a module"
    );
}

#[test]
fn entry_file_stems_are_main_test_and_mustfail_only() {
    assert!(is_entry_file_stem("main"));
    assert!(is_entry_file_stem("test_codegen"));
    assert!(is_entry_file_stem("mustfail_ast_compare"));
    assert!(!is_entry_file_stem("self_test"));
    assert!(!is_entry_file_stem("samples"));
    assert!(!is_entry_file_stem("mainline"));
}

/// The mention scan matches the walk-root-relative path, not the bare filename:
/// two directories can hold a `helpers.tg` and only one of them be copied.
#[test]
fn a_build_mention_matches_the_path_and_not_the_bare_filename() {
    let texts = vec![
        "\t@cp src/compiler/driver/ffi/diagnostics_dev.tg src/compiler/driver/ffi/diagnostics.tg\n"
            .to_string(),
    ];
    let candidates = set(&[
        "driver/ffi/diagnostics_dev.tg",
        "parser/self_test.tg",
        "other/diagnostics_dev.tg",
    ]);
    assert_eq!(
        mentioned_in_build_files(&candidates, &texts),
        set(&["driver/ffi/diagnostics_dev.tg"])
    );
}

#[test]
fn no_build_text_mentions_nothing() {
    let candidates = set(&["driver/ffi/diagnostics_dev.tg"]);
    assert!(mentioned_in_build_files(&candidates, &[]).is_empty());
}

#[test]
fn a_recipe_carrying_file_is_recognised_by_name_or_extension() {
    assert!(is_build_file(Path::new("/repo/Makefile")));
    assert!(is_build_file(Path::new("/repo/make/native.mk")));
    assert!(is_build_file(Path::new("/repo/scripts/build.sh")));
    assert!(is_build_file(Path::new("/repo/scripts/gen.py")));
    assert!(is_build_file(Path::new("/repo/.github/workflows/ci.yml")));
    assert!(!is_build_file(Path::new("/repo/README.md")));
    assert!(!is_build_file(Path::new("/repo/src/compiler/main.tg")));
}
