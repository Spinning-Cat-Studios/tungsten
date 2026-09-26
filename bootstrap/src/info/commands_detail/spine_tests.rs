//! Tests for `info type spine` (ADR 7.9.26c).
//!
//! Two jobs, and the first is the reason this ADR exists. ADR 4.9.26b's §1.1
//! pair — a named nested record against an anonymous tuple — is the premise the
//! reference prose publishes, so it is asserted here rather than only written
//! down (7.9.26c D4). The P0 table in
//! `docs/repo-memory/elaboration-pipeline.md` § What a record's product spine
//! is made of is measured by [`field_type_forms_splice_or_refer_as_documented`],
//! so a form whose behaviour changes fails a test instead of aging quietly into
//! a wrong doc.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::{render_spine_report, spine_components};
use crate::info::elaborate_for_info;

/// The §1.1 pair, plus the neighbours P0 measured, in one module.
const PROBE_SOURCE: &str = "\
type Inner = { left: Nat, right: Nat }
type Pair2 = (Nat, Nat)
type Box<T> = { item: T }
type Choice = Yes(Nat) | No(Nat)
type OneCtor = Mk(Nat, Nat)
type Chain = Link(Nat, Chain) | End

type NamedTail = { head: Nat, mid: Nat, tail: Inner }
type TupleTail = { head: Nat, mid: Nat, tail: (Nat, Nat) }
type AliasTail = { head: Nat, mid: Nat, tail: Pair2 }
type GenericTail = { head: Nat, mid: Nat, tail: Box<Nat> }
type SumTail = { head: Nat, mid: Nat, tail: Choice }
type UnitTail = { head: Nat, mid: Nat, tail: Unit }
type FnTail = { head: Nat, mid: Nat, tail: Nat -> Nat }
type Tuple3Tail = { head: Nat, mid: Nat, tail: (Nat, Nat, Nat) }
type OneCtorTail = { head: Nat, mid: Nat, tail: OneCtor }
type ChainTail = { head: Nat, mid: Nat, tail: Chain }
type MidTuple = { head: (Nat, Nat), mid: Nat, tail: Nat }
type OneField = { only: (Nat, Nat) }

fn main() -> Nat { 0 }
";

fn elaborate_fixture(source: &str) -> (TempDir, tungsten_bootstrap::driver::ProjectOutput) {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("fixture.tg");
    fs::write(&path, source).unwrap();
    let project = elaborate_for_info(&path, false, 20).expect("fixture elaborates");
    (dir, project)
}

/// Pull the two reported counts back out of a report, so a test asserts the
/// numbers a reader would read rather than a substring that could drift.
fn counts(report: &str) -> (usize, usize) {
    let read = |label: &str| -> usize {
        report
            .lines()
            .find(|line| line.starts_with(label))
            .unwrap_or_else(|| panic!("no {label:?} line in:\n{report}"))
            .rsplit(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    (read("Declared fields:"), read("Encoded spine:"))
}

/// **7.9.26c AC 2** — the §1.1 rows of ADR 4.9.26b, asserted rather than only
/// published (D4). A NAMED field type leaves field count equal to spine length;
/// an anonymous tuple in the tail position does not.
#[test]
fn a_named_field_type_refers_and_an_anonymous_tuple_splices() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);

    let named = render_spine_report("NamedTail", &project).expect("NamedTail is a record");
    assert_eq!(
        counts(&named),
        (3, 3),
        "a named nested record stays a reference:\n{named}"
    );

    let tuple = render_spine_report("TupleTail", &project).expect("TupleTail is a record");
    assert_eq!(
        counts(&tuple),
        (3, 4),
        "an anonymous tuple tail is spliced into the spine:\n{tuple}"
    );
}

/// **7.9.26c AC 3** — the pure reporting function prints both numbers for the
/// §1.1 pair, with the splice line that distinguishes them.
#[test]
fn the_report_prints_both_numbers_and_the_splice_line() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);

    let named = render_spine_report("NamedTail", &project).unwrap();
    assert!(named.contains("Declared fields: 3"), "{named}");
    assert!(named.contains("Encoded spine:   3"), "{named}");
    assert!(
        named.contains("Spine equals field count"),
        "the equal arm must say so in words too:\n{named}"
    );

    let tuple = render_spine_report("TupleTail", &project).unwrap();
    assert!(tuple.contains("Declared fields: 3"), "{tuple}");
    assert!(tuple.contains("Encoded spine:   4"), "{tuple}");
    assert!(tuple.contains("Spine exceeds field count by 1"), "{tuple}");
    // D2: counts, never a reachability verdict.
    assert!(
        !tuple.to_lowercase().contains("reachable"),
        "the report must not editorialise about reachability:\n{tuple}"
    );
}

/// The whole report for the splicing half of the §1.1 pair, exactly.
///
/// The two counts are the finding, but they are not the report: the field
/// listing and the spine listing are how a reader checks the counts rather than
/// trusting them, and a summary-line assertion cannot see either going missing.
#[test]
fn the_report_lists_both_decompositions_verbatim() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);
    let report = render_spine_report("TupleTail", &project).unwrap();
    let expected = "\
Record Spine: TupleTail
═══════════════════════

Declared fields: 3
Encoded spine:   4

Declared fields (source order):
  0: head: Nat
  1: mid: Nat
  2: tail: (Nat × Nat)

Encoded spine components (right-nested; field i is snd^i then fst):
  0: Nat
  1: Nat
  2: Nat
  3: Nat

Spine exceeds field count by 1: a field type that is structurally a product is \
spliced into the spine rather than referenced.
";
    assert_eq!(report, expected);
}

/// The referring half, where the two listings differ in a way the counts alone
/// do not show: the spine's last component is the NAME `Inner`, not its body.
#[test]
fn a_referring_tail_shows_the_name_in_the_spine_slot() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);
    let report = render_spine_report("NamedTail", &project).unwrap();
    assert!(report.contains("  2: tail: Inner"), "{report}");
    assert!(
        report.contains("Encoded spine components (right-nested; field i is snd^i then fst):\n  0: Nat\n  1: Nat\n  2: Inner\n"),
        "the spine's last slot is the reference itself:\n{report}"
    );
}

/// **7.9.26c AC 1** — the P0 table, measured. Each row is `(type, declared
/// fields, encoded spine)`; the doc paragraph states the same rows in prose, so
/// a change to either has to be a change to both.
#[test]
fn field_type_forms_splice_or_refer_as_documented() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);

    // Forms that stay a REFERENCE — spine length equals the field count.
    for name in [
        "NamedTail",   // a named record
        "GenericTail", // a generic instantiation, App("Box", [Nat])
        "SumTail",     // a named multi-constructor ADT: inlined, but as a Sum
        "UnitTail",    // Unit
        "FnTail",      // an arrow
        "ChainTail",   // a recursive ADT: inlined, but under a μ-binder
        "MidTuple",    // a tuple in a NON-final field sits in a `fst` slot
    ] {
        let report = render_spine_report(name, &project).expect("record");
        assert_eq!(
            counts(&report),
            (3, 3),
            "{name} should not splice:\n{report}"
        );
    }

    // Forms that SPLICE — a product at the head of the tail field's encoding.
    for (name, expected) in [
        ("TupleTail", (3, 4)),   // an anonymous tuple
        ("AliasTail", (3, 4)),   // an alias to a tuple — the one a reader guesses wrong
        ("OneCtorTail", (3, 4)), // a single-constructor ADT whose body is a product
        ("Tuple3Tail", (3, 5)),  // a 3-tuple splices two extra components
        ("OneField", (1, 2)),    // a one-field record IS its field's encoding
    ] {
        let report = render_spine_report(name, &project).expect("record");
        assert_eq!(counts(&report), expected, "{name} should splice:\n{report}");
    }
}

/// The walk follows the RIGHT operand only. `MidTuple` is the case that proves
/// it: a product sitting in a `fst` slot is one component, not two.
#[test]
fn the_walk_descends_only_the_right_operand() {
    use tungsten_core::types::Type;

    let nested_left = Type::Product(
        Box::new(Type::Product(Box::new(Type::Nat), Box::new(Type::Nat))),
        Box::new(Type::Bool),
    );
    assert_eq!(spine_components(&nested_left).len(), 2);

    let nested_right = Type::Product(
        Box::new(Type::Nat),
        Box::new(Type::Product(Box::new(Type::Nat), Box::new(Type::Bool))),
    );
    assert_eq!(spine_components(&nested_right).len(), 3);

    // A non-product is a spine of one, not of zero.
    assert_eq!(spine_components(&Type::Nat).len(), 1);
}

/// All three arms of the summary line, including the one no encoding should
/// reach. A spine SHORTER than the field count would mean the encoding and the
/// declaration disagree; the arm exists so that state reads as a fault rather
/// than as a `+0`, and it is asserted here because no fixture can produce it.
#[test]
fn the_summary_line_names_which_way_the_counts_differ() {
    assert!(super::splice_summary(3, 4).contains("exceeds field count by 1"));
    assert!(super::splice_summary(3, 5).contains("exceeds field count by 2"));
    assert!(super::splice_summary(3, 3).contains("equals field count"));
    let short = super::splice_summary(3, 2);
    assert!(short.contains("SHORTER"), "{short}");
    assert!(short.contains("by 1"), "{short}");
}

/// **7.9.26c AC 5** — a non-record type is refused by name and kind, not
/// answered with a zero.
#[test]
fn a_non_record_type_is_refused_rather_than_counted() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);

    let adt = render_spine_report("Choice", &project).unwrap_err();
    assert!(adt.contains("is an ADT"), "{adt}");
    assert!(
        !adt.contains(": 0"),
        "a refusal must not print a count: {adt}"
    );

    let alias = render_spine_report("Pair2", &project).unwrap_err();
    assert!(alias.contains("is a type alias"), "{alias}");
}

/// **7.9.26c AC 5** — a parameterized record has no cached encoding, so there
/// is no spine to measure; say that rather than reporting a spine of one.
#[test]
fn a_parameterized_record_reports_no_cached_encoding() {
    let (_dir, project) = elaborate_fixture(PROBE_SOURCE);

    let refusal = render_spine_report("Box", &project).unwrap_err();
    assert!(refusal.contains("no cached encoding"), "{refusal}");
    assert!(refusal.contains("Box"), "{refusal}");
}

/// **7.9.26c AC 5** — an unknown name exits non-zero through the command shell.
#[test]
fn cmd_rejects_unknown_type() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fixture.tg");
    fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    let exit_code = super::cmd_info_type_spine("Ghost", &path, false, 20);
    assert_eq!(exit_code, std::process::ExitCode::FAILURE);
}

/// The success path through the command shell, so the wiring from
/// `render_spine_report` to an exit code is not asserted only in the negative.
#[test]
fn cmd_succeeds_on_a_record() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fixture.tg");
    fs::write(&path, PROBE_SOURCE).unwrap();
    let exit_code = super::cmd_info_type_spine("TupleTail", &path, false, 20);
    assert_eq!(exit_code, std::process::ExitCode::SUCCESS);
}

/// A non-record reaches `FAILURE` through the shell too — the `Err` arm of the
/// dispatch, which the pure tests above cannot reach.
#[test]
fn cmd_rejects_a_non_record_type() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fixture.tg");
    fs::write(&path, PROBE_SOURCE).unwrap();
    let exit_code = super::cmd_info_type_spine("Choice", &path, false, 20);
    assert_eq!(exit_code, std::process::ExitCode::FAILURE);
}
