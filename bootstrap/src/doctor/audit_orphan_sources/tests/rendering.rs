//! The report, over hand-built values — chiefly the three ways a census can be
//! vacuous and must not render like a clean tree (AC 3).

use super::*;

/// Zero files walked must not render like zero stranded files.
#[test]
fn an_empty_walk_is_a_finding_and_not_a_clean_report() {
    let empty = render_orphans(&OrphanSources {
        build_files_scanned: 1,
        ..OrphanSources::default()
    });
    let clean = render_orphans(&OrphanSources {
        files_walked: 310,
        entry_files_read: 24,
        modules_subtracted: 307,
        build_files_scanned: 42,
        ..OrphanSources::default()
    });

    assert!(
        empty.contains("no .tg file was walked"),
        "the empty walk must say so: {empty}"
    );
    assert!(!clean.contains("no .tg file was walked"), "{clean}");
    assert_ne!(empty, clean);
    assert!(clean.contains("stranded: none"), "{clean}");
    assert!(
        !empty.contains("stranded: none"),
        "an unrun census must not claim a clean class: {empty}"
    );
}

/// The other two vacuity directions, each of which empties a class for a
/// reason that has nothing to do with the tree being healthy.
#[test]
fn subtracting_nothing_and_scanning_nothing_each_get_their_own_warning() {
    let nothing_subtracted = render_orphans(&OrphanSources {
        files_walked: 3,
        build_files_scanned: 1,
        ..OrphanSources::default()
    });
    assert!(
        nothing_subtracted.contains("no module was subtracted"),
        "{nothing_subtracted}"
    );
    assert!(
        !nothing_subtracted.contains("no build file was scanned"),
        "{nothing_subtracted}"
    );

    let nothing_scanned = render_orphans(&OrphanSources {
        files_walked: 3,
        modules_subtracted: 2,
        ..OrphanSources::default()
    });
    assert!(
        nothing_scanned.contains("no build file was scanned"),
        "{nothing_scanned}"
    );
    assert!(
        !nothing_scanned.contains("no module was subtracted"),
        "{nothing_scanned}"
    );
}

#[test]
fn an_unparseable_entry_file_is_named_in_the_report() {
    let rendered = render_orphans(&OrphanSources {
        files_walked: 3,
        entry_files_read: 1,
        modules_subtracted: 2,
        build_files_scanned: 4,
        unreadable_entries: set(&["test_broken.tg"]),
        ..OrphanSources::default()
    });
    assert!(rendered.contains("test_broken.tg"), "{rendered}");
    assert!(rendered.contains("subtracted nothing"), "{rendered}");
}

/// Each class prints its members and its count when it has any.
#[test]
fn a_populated_class_lists_its_members_and_its_count() {
    let rendered = render_orphans(&OrphanSources {
        files_walked: 5,
        entry_files_read: 1,
        modules_subtracted: 2,
        build_files_scanned: 4,
        stranded: set(&["parser/self_test.tg", "parser/tests/samples.tg"]),
        build_swapped: set(&["driver/ffi/diagnostics_dev.tg"]),
        entry_roots: set(&["main.tg"]),
        ..OrphanSources::default()
    });
    assert!(rendered.contains("stranded: 2 file(s)"), "{rendered}");
    assert!(rendered.contains("  parser/self_test.tg"), "{rendered}");
    assert!(rendered.contains("  parser/tests/samples.tg"), "{rendered}");
    assert!(rendered.contains("build-swapped: 1 file(s)"), "{rendered}");
    assert!(rendered.contains("entry files: 1 file(s)"), "{rendered}");
    assert!(
        rendered.contains(
            "5 .tg file(s) walked, 1 entry file(s) read, 2 module(s) subtracted, \
             4 build file(s) scanned"
        ),
        "{rendered}"
    );
}
