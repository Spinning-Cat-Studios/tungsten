//! The `check` verdict line (ADR 18.9.26g AC2).

use tungsten_core::terms::analysis::SorryCounts;

use super::check_verdict_line;

// 18.9.26g AC2: a program with no hole reads all OK.
#[test]
fn a_hole_free_program_reads_all_ok() {
    assert_eq!(
        check_verdict_line("a.tg", 3, &SorryCounts::default()),
        "✓ a.tg: 3 definition(s), all OK"
    );
}

// 18.9.26g AC2: both counts print, and unclassified folds into synthesised.
#[test]
fn the_verdict_line_carries_both_counts() {
    let sorry = SorryCounts {
        authored: 2,
        synthesised: 3,
        unclassified: 1,
    };
    assert_eq!(
        check_verdict_line("a.tg", 6, &sorry),
        "⚠ a.tg: 6 definition(s), contains sorry (2 authored, 4 synthesised)"
    );
}

// 18.9.26g AC2: one hole of any class is enough to leave all OK.
#[test]
fn a_single_hole_of_any_class_is_reported() {
    let one_of_each = [
        SorryCounts {
            authored: 1,
            ..SorryCounts::default()
        },
        SorryCounts {
            synthesised: 1,
            ..SorryCounts::default()
        },
        SorryCounts {
            unclassified: 1,
            ..SorryCounts::default()
        },
    ];
    for sorry in one_of_each {
        let line = check_verdict_line("a.tg", 1, &sorry);
        assert!(line.contains("contains sorry"), "{line}");
    }
}
