//! Integration tests for the cache diagnostic tooling (ADR 4.7.26d):
//! `tungsten cache inspect` and `tungsten diff cache`.
//!
//! The inspect tests drive the library entry point `driver::inspect_cache`
//! directly (evaluator path — no LLVM). The `diff cache` parity test spawns the
//! compiled binary (its command handler lives in the binary crate). Both reuse
//! the 4.7.26c fixture — `main` + a `test_` fn — and an isolated cache dir, so a
//! cold `check`/`run` populates the same `.tungsten` a later step reads.

use std::path::{Path, PathBuf};
use std::process::Command;

use tungsten_bootstrap::driver::{
    self, CacheEntryKind, Mode, ModuleCacheRow, PipelineOpts, PipelineResult,
};

const FIXTURE: &str = "\
fn main() -> Nat { 0 }
fn helper() -> Nat { 42 }
fn test_trivial() -> () { }
";

/// Write the fixture into a fresh tempdir; the `TempDir` guard must outlive use.
fn fixture_in_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prog.tg");
    std::fs::write(&path, FIXTURE).unwrap();
    (dir, path)
}

fn check(path: &Path) {
    let opts = PipelineOpts {
        mode: Mode::Check,
        verbose: false,
        dump_types: false,
    };
    assert!(
        matches!(
            driver::run_file_with_options(path, &opts, false, 0).unwrap(),
            PipelineResult::Checked { .. }
        ),
        "check should succeed on the fixture"
    );
}

fn only_row(rows: &[ModuleCacheRow]) -> &ModuleCacheRow {
    assert_eq!(
        rows.len(),
        1,
        "single-file fixture has one module: {rows:?}"
    );
    &rows[0]
}

// ── `cache inspect` ─────────────────────────────────────────────────────

/// AC1: after a warm `check` (writes a signature-only entry), inspect lists the
/// module as `signature-only`, a non-zero def count, and `serves_bodies = No`
/// for run/test — the 4.7.26c hazard.
#[test]
fn inspect_surfaces_signature_only_hazard() {
    let (_dir, path) = fixture_in_tempdir();
    check(&path); // cold check populates the signature cache

    let rows = driver::inspect_cache(&path, false).expect("inspect should succeed");
    let row = only_row(&rows);
    assert_eq!(
        row.kind,
        CacheEntryKind::SignatureOnly,
        "a check writes the bodyless signature tier"
    );
    assert_eq!(row.def_count, Some(3), "fixture has 3 definitions");
    assert!(
        !CacheEntryKind::SignatureOnly.serves_bodies(),
        "signature-only must NOT serve run/test bodies (the hazard)"
    );
}

/// AC3: a fresh / uncached project reports `(uncached)` and no hazard for every
/// module — no false positive.
#[test]
fn inspect_fresh_project_is_uncached_no_hazard() {
    let (_dir, path) = fixture_in_tempdir();
    // No prior build → no `.tungsten` elab entries.
    let rows = driver::inspect_cache(&path, false).expect("inspect should succeed");
    let row = only_row(&rows);
    assert_eq!(row.kind, CacheEntryKind::Uncached);
    assert_eq!(row.def_count, None);
    assert!(
        row.kind.serves_bodies(),
        "an uncached module is elaborated fresh → serves bodies"
    );
}

/// A `def_count` is only reported once an entry exists; the tier is intrinsic.
#[test]
fn inspect_kind_is_intrinsic_to_entry() {
    let (_dir, path) = fixture_in_tempdir();
    // Cold inspect: uncached.
    assert_eq!(
        driver::inspect_cache(&path, false).unwrap()[0].kind,
        CacheEntryKind::Uncached
    );
    // After a check: signature-only.
    check(&path);
    assert_eq!(
        driver::inspect_cache(&path, false).unwrap()[0].kind,
        CacheEntryKind::SignatureOnly
    );
}

/// AC2: `--json` emits one structured record per module that deserializes with
/// the documented fields; the hazard record round-trips.
#[test]
fn inspect_json_is_parseable() {
    let (_dir, path) = fixture_in_tempdir();
    check(&path);

    let bin = env!("CARGO_BIN_EXE_tungsten");
    let out = Command::new(bin)
        .args([
            "cache",
            "inspect",
            path.to_str().unwrap(),
            "--mode",
            "run",
            "--json",
        ])
        .output()
        .expect("spawn tungsten cache inspect --json");
    assert!(out.status.success(), "inspect --json should exit 0");

    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("output must be valid JSON");
    let records = parsed.as_array().expect("top level is an array");
    assert_eq!(records.len(), 1, "one record per module");
    let rec = &records[0];
    assert_eq!(rec["entry_kind"], "signature-only");
    assert_eq!(rec["def_count"], 3);
    assert_eq!(rec["serves_bodies"], false);
    assert!(rec["module"].is_string());
}

// ── `diff cache` ────────────────────────────────────────────────────────

/// AC4: on a healthy compiler, cold and warm agree — exit 0 — for both modes.
#[test]
fn diff_cache_parity_on_healthy_compiler() {
    let (_dir, path) = fixture_in_tempdir();
    let bin = env!("CARGO_BIN_EXE_tungsten");
    for mode in ["run", "test"] {
        let status = Command::new(bin)
            .args(["diff", "cache", path.to_str().unwrap(), "--mode", mode])
            .status()
            .expect("spawn tungsten diff cache");
        assert_eq!(
            status.code(),
            Some(0),
            "cold and warm must agree (parity) for --mode {mode}"
        );
    }
}

/// The `--gate` form propagates the same parity exit on a healthy build.
#[test]
fn diff_cache_gate_is_zero_on_parity() {
    let (_dir, path) = fixture_in_tempdir();
    let bin = env!("CARGO_BIN_EXE_tungsten");
    let status = Command::new(bin)
        .args(["diff", "cache", path.to_str().unwrap(), "--gate"])
        .status()
        .expect("spawn tungsten diff cache --gate");
    assert_eq!(status.code(), Some(0));
}

// ── discovery (ADR 4.7.26d) ─────────────────────────────────────────────

/// AC6: both tools are discoverable from the stale-cache symptom cluster via
/// `doctor suggest-tools`, alongside the `cache clean` remedy.
#[test]
fn suggest_tools_lists_cache_diagnostics() {
    let bin = env!("CARGO_BIN_EXE_tungsten");
    let out = Command::new(bin)
        .args(["doctor", "suggest-tools", "no tests found"])
        .output()
        .expect("spawn tungsten doctor suggest-tools");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("cache inspect"),
        "missing cache inspect: {text}"
    );
    assert!(text.contains("diff cache"), "missing diff cache: {text}");
    assert!(text.contains("cache clean"), "missing cache clean: {text}");
}
