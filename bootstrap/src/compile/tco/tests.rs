//! Fixture-driven integration tests for `tco-coverage` (ADR 1.7.26b §5).
//!
//! Each test runs **real** codegen (cost 4 — requires the `codegen` feature and
//! LLVM) over an isolated fixture under `tests/fixtures/tco/` and asserts the
//! structured decision + risk rank. Fixtures isolate one behaviour each so a
//! failure points at *this* ADR's logic, not moving production source.

use std::path::PathBuf;

use tungsten_codegen::Decision;

use super::collect::collect_musttail_decisions;
use super::risk::{build_coverage, FunctionCoverage, Risk};

/// Absolute path to a fixture by stem.
fn fixture(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tco")
        .join(format!("{stem}.tg"))
}

/// Run codegen over a fixture and return its aggregated coverage rows.
fn coverage(stem: &str) -> Vec<FunctionCoverage> {
    let run = collect_musttail_decisions(&fixture(stem), false, 20)
        .unwrap_or_else(|e| panic!("codegen collect failed for {stem}: {e}"));
    build_coverage(&run.decisions, &run.fn_types)
}

/// Find the row for `name`, panicking with context if absent.
fn row<'a>(rows: &'a [FunctionCoverage], name: &str) -> &'a FunctionCoverage {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no coverage row for '{name}' in {rows:?}"))
}

#[test]
fn struct_return_skip_is_high() {
    let rows = coverage("struct_return_skip");
    let r = row(&rows, "countdown");
    // ADR 1.7.26c P1: countdown is a Class R function (return-only struct param).
    // With P1, it now creates a $direct_mt entry with sret, so it should DECOMPOSE
    // (even though it has no flattenable struct params, the $direct_mt with sret
    // enables musttail on the constant-stack $direct_mt entry).
    assert_eq!(r.decision, Decision::Decompose);
    assert_eq!(r.risk, Risk::Low);
}

#[test]
fn flat_param_decomposes_to_low() {
    let rows = coverage("flat_param_emit");
    let r = row(&rows, "scan");
    assert!(r.decision.is_constant_stack());
    assert_eq!(r.risk, Risk::Low);
}

#[test]
fn nonflat_param_decomposes_via_indirect() {
    // Fixture historically named `nonflat_param_skip` (pre-1.7.26e it SKIPped).
    // ADR 1.7.26e: a non-flattenable struct param is now passed INDIRECT (a
    // caller-owned buffer ptr), so `descend` reaches a musttail `$direct_mt`
    // → DECOMPOSE / LOW (was SKIP / HIGH under NON_FLATTENABLE_PARAM).
    let rows = coverage("nonflat_param_skip");
    let r = row(&rows, "descend");
    assert_eq!(r.decision, Decision::Decompose);
    assert_eq!(r.risk, Risk::Low);
}

#[test]
fn collection_driven_skip_is_high() {
    let rows = coverage("collection_high");
    let r = row(&rows, "sum_pairs");
    // ADR 1.7.26c P1: sum_pairs has struct return but only scalar/List params.
    // With P1, it now DECOMPOSE/LOW (sret $direct_mt, List is already ptr).
    assert_eq!(r.decision, Decision::Decompose);
    assert_eq!(r.risk, Risk::Low);
    assert!(r.driver.contains("List"));
}

#[test]
fn bounded_finite_adt_driver_is_med() {
    let rows = coverage("bounded_med");
    let r = row(&rows, "classify");
    // ADR 1.7.26c P1: classify is a Class R function (struct return, scalar params).
    // With P1, it now DECOMPOSE/LOW (sret $direct_mt).
    assert_eq!(r.decision, Decision::Decompose);
    assert_eq!(r.risk, Risk::Low);
}

#[test]
fn unclassifiable_driver_is_unknown() {
    let rows = coverage("unknown_driver");
    let r = row(&rows, "flip");
    // ADR 1.7.26c P1: flip has struct return with scalar params.
    // With P1, it now DECOMPOSE/LOW (sret $direct_mt).
    assert_eq!(r.decision, Decision::Decompose);
    assert_eq!(r.risk, Risk::Low);
}

#[test]
fn mixed_file_sorts_high_before_low_with_correct_totals() {
    let rows = coverage("mixed_file");
    // ADR 1.7.26c P1: pair_up (struct return, scalar params) is now DECOMPOSE/LOW.
    // add_up (scalar return) is still EMIT/LOW.
    let pair = row(&rows, "pair_up");
    assert_eq!(pair.decision, Decision::Decompose);
    assert_eq!(pair.risk, Risk::Low);
    let emit = row(&rows, "add_up");
    assert!(emit.decision.is_constant_stack());
    assert_eq!(emit.risk, Risk::Low);
}

#[test]
fn json_contract_has_stable_enums_and_totals() {
    let run = collect_musttail_decisions(&fixture("struct_return_skip"), false, 20).unwrap();
    let rows = build_coverage(&run.decisions, &run.fn_types);
    let json = super::render::render_json(&rows, &run.decisions);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    // ADR 1.7.26c P1: countdown is now DECOMPOSE/LOW (Class R sret $direct_mt).
    // Find countdown in the functions list.
    let countdown = v["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "countdown")
        .expect("countdown not found");
    assert_eq!(countdown["decision"], "DECOMPOSE");
    assert_eq!(countdown["risk"], "LOW");
    // DECOMPOSE counts as "emit" (constant-stack). skip_sites should be 0.
    assert_eq!(countdown["sites"]["skip"], 0);
    // emit_sites is ≥ 1 (the self-recursive tail call).
    assert!(countdown["sites"]["emit"].as_u64().unwrap() >= 1);

    // Per-site records + totals present.
    assert!(v["sites"].is_array());
    // At least countdown's emit sites should be in the totals.
    assert!(v["totals"]["emit"].as_u64().unwrap() >= 1);
}
