//! Golden-style tests for `info type size` (ADR 8.7.26a §2.3): exact-output
//! assertions on fixture ADTs with known counts. Lives here rather than
//! `tests/golden/` because the golden runner's categories cover
//! check/run/error/compile/test only; an exact string assertion is the same
//! verification at cost 3.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::render_type_size_report;
use crate::info::elaborate_for_info;

fn elaborate_fixture(source: &str) -> (TempDir, tungsten_bootstrap::driver::ProjectOutput) {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("fixture.tg");
    fs::write(&path, source).unwrap();
    let project = elaborate_for_info(&path, false, 20).expect("fixture elaborates");
    (dir, project)
}

#[test]
fn recursive_adt_report_matches_golden() {
    // NatList encodes as μα_NatList. (Unit + (Nat × α_NatList)):
    // Mu + Sum + Unit + Product + Nat + TyVar = 6 nodes, depth 4, k = 1.
    let (_dir, project) =
        elaborate_fixture("type NatList = Empty | More(Nat, NatList)\nfn main() -> Nat { 0 }");
    let report = render_type_size_report("NatList", &project);
    let expected = "\
Type Size: NatList
══════════════════

Stored encoding tree:
  node count: 6
  max depth:  4
  μ-binder chain: α_NatList
  α-occurrences per binder (the ∏ kᵢ factors):
    α_NatList: 1

Per-variant stored field-tree node counts:
  Empty: 0 node(s) across 0 field(s)
  More: 2 node(s) across 2 field(s)

(shape rather than size: `tungsten info type encoding NatList <file>`)
";
    assert_eq!(report, expected);
}

#[test]
fn non_recursive_adt_reports_no_binders() {
    let (_dir, project) =
        elaborate_fixture("type Color = Red | Green | Blue\nfn main() -> Nat { 0 }");
    let report = render_type_size_report("Color", &project);
    assert!(
        report.contains("μ-binders:  (none — not recursive)"),
        "{report}"
    );
    assert!(
        report.contains("Red: 0 node(s) across 0 field(s)"),
        "{report}"
    );
}

#[test]
fn parameterized_adt_reports_uncached_encoding() {
    let (_dir, project) =
        elaborate_fixture("type List<T> = Nil | Cons(T, List<T>)\nfn main() -> Nat { 0 }");
    let report = render_type_size_report("List", &project);
    assert!(report.contains("Stored encoding: (none cached"), "{report}");
    assert!(
        report.contains("Per-variant stored field-tree node counts:"),
        "per-variant counts still render without a cached encoding: {report}"
    );
}

#[test]
fn cmd_rejects_unknown_type() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fixture.tg");
    fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
    let result = super::cmd_info_type_size("Ghost", &path, false, 20);
    assert_eq!(result, std::process::ExitCode::FAILURE);
}
