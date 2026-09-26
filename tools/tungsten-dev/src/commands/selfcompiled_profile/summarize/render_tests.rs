//! Tests for `render.rs` (the `--by-class` / `--growth` renderers).
//! Kept in a `#[path]` sibling so `render.rs` stays under the 400-LOC gate.

use super::*;
use crate::commands::selfcompiled_profile::summarize::parse_profile_log;

const SAMPLE: &str = "\
  [alloc-profile] ▶ phaseB:start | total=128,934,553 allocs=2,540,113 | mu=122,002,720 env=4,754,412 str=2,177,421
  [arena] types=14888 deep=146MB terms=0 deep=0MB ctxs=0 slab=1MB vmrss=372MB
  [alloc-profile] ▶ phaseB:module lexer::scanner | total=200,000,000 allocs=3,000,000 | mu=190,000,000 env=5,000,000 str=2,200,000
  [arena] types=20000 deep=200MB terms=100 deep=50MB ctxs=0 slab=2MB vmrss=500MB
  [alloc-profile] ▶ phaseB:module parser::exprs | total=400,000,000 allocs=5,000,000 | mu=380,000,000 env=6,000,000 str=2,300,000
  [arena] types=30000 deep=300MB terms=2000 deep=2250MB ctxs=0 slab=3MB vmrss=3000MB
";

#[test]
fn deltas_attribute_to_the_earlier_marker() {
    let snaps = parse_profile_log(SAMPLE);
    let summary = render_summary(&snaps, 10, false, false);
    // lexer::scanner's delta is the growth BETWEEN its marker and the
    // parser::exprs marker: 3000-500=2500 RSS, 2250-50=2200 terms.
    let scanner_row = summary
        .lines()
        .find(|l| l.contains("lexer::scanner"))
        .unwrap();
    assert!(scanner_row.contains("dRSS=  2500MB"), "{scanner_row}");
    assert!(scanner_row.contains("dTerms=  2200MB"), "{scanner_row}");
    // self-host alloc delta 400M-200M = 200M ⇒ 190 MB (guards the /MB scaling).
    assert!(scanner_row.contains("dSelfHost=  190MB"), "{scanner_row}");
    // phaseB:start is not a module marker — no delta row for it, so
    // exactly one module delta exists (parser::exprs has no successor).
    assert!(summary.contains("1 module deltas"), "{summary}");
}

#[test]
fn terminal_line_reports_last_snapshot() {
    let snaps = parse_profile_log(SAMPLE);
    let summary = render_summary(&snaps, 10, false, false);
    assert!(summary.contains("terminal: at `phaseB:module parser::exprs`"));
    assert!(summary.contains("vmrss=3000MB"), "{summary}");
}

#[test]
fn default_output_omits_class_columns_and_growth() {
    let snaps = parse_profile_log(SAMPLE);
    let summary = render_summary(&snaps, 10, false, false);
    assert!(!summary.contains("dMu="), "class columns leaked: {summary}");
    assert!(!summary.contains("per-class cumulative"), "{summary}");
    assert!(!summary.contains("growth (walk order)"), "{summary}");
}

#[test]
fn tolerates_missing_arena_lines_without_panicking() {
    let text = "\
  [alloc-profile] ▶ phaseB:module a | total=1,000,000 allocs=10 | mu=1 env=1 str=1
  [alloc-profile] ▶ phaseB:module b | total=3,000,000 allocs=20 | mu=2 env=1 str=1
  [arena] types=1 deep=0MB terms=0 deep=0MB ctxs=0 slab=0MB vmrss=naMB
";
    let snaps = parse_profile_log(text);
    let summary = render_summary(&snaps, 5, false, false);
    // Module `a` still gets a self-host alloc delta row without arena data.
    assert!(
        summary
            .lines()
            .any(|l| l.contains("  a ") && l.contains("dSelfHost=")),
        "{summary}"
    );
}

#[test]
fn by_class_adds_delta_columns() {
    let snaps = parse_profile_log(SAMPLE);
    let summary = render_summary(&snaps, 10, true, false);
    let scanner_row = summary
        .lines()
        .find(|l| l.contains("lexer::scanner"))
        .unwrap();
    // mu delta: 380M-190M = 190M ≈ 181 MB; env delta: 6M-5M = 1M ≈ 0 MB.
    assert!(scanner_row.contains("dMu=   181MB"), "{scanner_row}");
    assert!(scanner_row.contains("dEnv=     0MB"), "{scanner_row}");
    assert!(scanner_row.contains("dRef=     0MB"), "{scanner_row}");
}

#[test]
fn by_class_terminal_table_shows_shares() {
    let snaps = parse_profile_log(SAMPLE);
    let summary = render_summary(&snaps, 10, true, false);
    assert!(summary.contains("per-class cumulative"), "{summary}");
    // terminal total=400M, mu=380M ⇒ 95.00%; str=2.3M ⇒ 0.57%.
    let mu_row = summary.lines().find(|l| l.contains("mu ")).unwrap();
    assert!(mu_row.contains("95.00%"), "{mu_row}");
    let str_row = summary
        .lines()
        .find(|l| l.trim_start().starts_with("str "))
        .unwrap();
    assert!(str_row.contains("0.57%"), "{str_row}");
}

/// Synthetic walk whose cumulative total = MB·k(k+1)/2, so each module's
/// self-host alloc delta is MB·(k+1) — proportional to its 1-based walk position,
/// the exact linear-in-index ramp ADR 23.7.26d convicted as O(M²).
fn linear_index_snapshots(n_modules: u64) -> Vec<MarkerSnapshot> {
    (0..n_modules)
        .map(|k| MarkerSnapshot {
            label: format!("phaseB:module m{k}"),
            alloc_total_bytes: MB * k * (k + 1) / 2,
            by_class: [0; NUM_CLASSES],
            arena: None,
        })
        .collect()
}

#[test]
fn growth_flags_linear_in_index_ramp_as_super_linear() {
    let snaps = linear_index_snapshots(64);
    let summary = render_summary(&snaps, 5, false, true);
    assert!(summary.contains("growth (walk order):"), "{summary}");
    assert!(
        summary.contains("SUPER-LINEAR (O(M²)-suspect)"),
        "{summary}"
    );
}

#[test]
fn growth_reports_exact_window_means_and_index_ratio() {
    // 16 modules ⇒ 15 deltas ⇒ window=min(30,7)=7 (odd ⇒ integer
    // centroids 4 and 12). Deltas are 1..15 MB, so first-7 mean = 4.0,
    // last-7 mean = 12.0, ratio 3.00×; centroid index ratio 12/4 = 3.0×.
    // Asserting these exact numbers pins the window/centroid/ratio
    // arithmetic (a mutated `/`, `-`, or `+` shifts a displayed value).
    let snaps = linear_index_snapshots(16);
    let summary = render_summary(&snaps, 2, false, true);
    assert!(
        summary.contains("first-7 mean 4.0 MB → last-7 mean 12.0 MB = 3.00×"),
        "{summary}"
    );
    assert!(summary.contains("≈ 12/4 ≈ 3.0×"), "{summary}");
}

#[test]
fn growth_flags_constant_ramp_as_flat() {
    // cumulative total = MB·k ⇒ every per-module delta = 1 MB (flat).
    let snaps: Vec<MarkerSnapshot> = (0..64)
        .map(|k| MarkerSnapshot {
            label: format!("phaseB:module m{k}"),
            alloc_total_bytes: MB * k,
            by_class: [0; NUM_CLASSES],
            arena: None,
        })
        .collect();
    let summary = render_summary(&snaps, 5, false, true);
    assert!(summary.contains("FLAT/LINEAR"), "{summary}");
}

#[test]
fn growth_verdict_classifies_the_three_regimes() {
    // 23.7.26d's numbers: 10.77× mean vs ~13× index ⇒ super-linear.
    assert_eq!(growth_verdict(10.77, 13.0), "SUPER-LINEAR (O(M²)-suspect)");
    // mean tracks index closely ⇒ super-linear.
    assert_eq!(growth_verdict(3.13, 3.13), "SUPER-LINEAR (O(M²)-suspect)");
    // mean ≈ 1 ⇒ per-module cost flat.
    assert!(growth_verdict(1.05, 13.0).starts_with("FLAT/LINEAR"));
    // grows, but well below index-proportional ⇒ sub-linear.
    assert!(growth_verdict(4.0, 13.0).starts_with("SUB-LINEAR"));
    // Exactly at the FLAT threshold (1.0 + 0.25): the `<` bound must
    // EXCLUDE it, so 1.25 is not FLAT — it is sub-index growth.
    assert!(growth_verdict(1.25, 13.0).starts_with("SUB-LINEAR"));
}

#[test]
fn growth_handles_too_few_modules() {
    let snaps = linear_index_snapshots(1); // 1 module ⇒ 0 deltas
    let summary = render_summary(&snaps, 5, false, true);
    assert!(summary.contains("too few to trend"), "{summary}");
}

#[test]
fn growth_at_two_deltas_with_zero_first_mean_is_infinite_not_nan() {
    // Three equal-total markers ⇒ 2 zero-valued deltas. This is the twin
    // boundary: n == 2 must still TREND (the `n < 2` guard excludes it),
    // and a zero first-window mean must take the divide-by-zero guard to
    // report `inf×`, never `NaN×`.
    let snaps: Vec<MarkerSnapshot> = (0..3)
        .map(|k| MarkerSnapshot {
            label: format!("phaseB:module m{k}"),
            alloc_total_bytes: 1_000_000,
            by_class: [0; NUM_CLASSES],
            arena: None,
        })
        .collect();
    let summary = render_summary(&snaps, 5, false, true);
    assert!(summary.contains("first-1 mean 0.0 MB"), "{summary}");
    assert!(!summary.contains("too few"), "n==2 must trend: {summary}");
    assert!(
        !summary.contains("NaN"),
        "zero mean must be inf, not NaN: {summary}"
    );
}

#[test]
fn by_class_zero_total_terminal_renders_zero_share_not_nan() {
    // A terminal marker with total=0 must take the divide-by-zero guard:
    // every class share renders 0.00%, never NaN.
    let snaps = vec![
        MarkerSnapshot {
            label: "phaseB:module a".into(),
            alloc_total_bytes: 0,
            by_class: [0; NUM_CLASSES],
            arena: None,
        },
        MarkerSnapshot {
            label: "phaseB:module b".into(),
            alloc_total_bytes: 0,
            by_class: [0; NUM_CLASSES],
            arena: None,
        },
    ];
    let summary = render_summary(&snaps, 5, true, false);
    assert!(
        summary.contains("per-class cumulative (of terminal self-host alloc=0MB)"),
        "{summary}"
    );
    assert!(summary.contains("0.00%"), "{summary}");
    assert!(!summary.contains("NaN"), "{summary}");
}
