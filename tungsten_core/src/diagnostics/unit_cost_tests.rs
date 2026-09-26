//! Tests for the unit-cost census report logic (ADR 8.7.26a §2.1).

use super::*;

fn record(name: &str, index: usize, secs: f64, alloc: u64) -> UnitCostRecord {
    UnitCostRecord {
        unit_name: name.to_string(),
        unit_index: index,
        wall_time_secs: secs,
        alloc_bytes: alloc,
    }
}

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

// ── threshold parsing ──

#[test]
fn parses_time_threshold() {
    assert_eq!(
        CostThreshold::parse("0.5s"),
        Ok(CostThreshold::WallTimeSecs(0.5))
    );
    assert_eq!(
        CostThreshold::parse("30s"),
        Ok(CostThreshold::WallTimeSecs(30.0))
    );
}

#[test]
fn parses_alloc_threshold_binary_multipliers() {
    assert_eq!(
        CostThreshold::parse("8GB"),
        Ok(CostThreshold::AllocBytes(8 * GIB))
    );
    assert_eq!(
        CostThreshold::parse("8GiB"),
        Ok(CostThreshold::AllocBytes(8 * GIB))
    );
    assert_eq!(
        CostThreshold::parse("512MB"),
        Ok(CostThreshold::AllocBytes(512 * MIB))
    );
    assert_eq!(
        CostThreshold::parse("512MiB"),
        Ok(CostThreshold::AllocBytes(512 * MIB))
    );
    assert_eq!(
        CostThreshold::parse("2kb"),
        Ok(CostThreshold::AllocBytes(2048))
    );
    assert_eq!(
        CostThreshold::parse("2KiB"),
        Ok(CostThreshold::AllocBytes(2048))
    );
}

#[test]
fn parses_fractional_alloc_threshold() {
    assert_eq!(
        CostThreshold::parse("1.5GB"),
        Ok(CostThreshold::AllocBytes(GIB + GIB / 2))
    );
}

#[test]
fn zero_is_a_valid_threshold() {
    // The negativity check is strict (< 0): a zero bound is legal and means
    // "every unit meets it".
    assert_eq!(
        CostThreshold::parse("0s"),
        Ok(CostThreshold::WallTimeSecs(0.0))
    );
    assert_eq!(
        CostThreshold::parse("0GB"),
        Ok(CostThreshold::AllocBytes(0))
    );
}

#[test]
fn rejects_malformed_thresholds() {
    assert!(CostThreshold::parse("fast").is_err());
    assert!(CostThreshold::parse("8").is_err());
    assert!(CostThreshold::parse("GB").is_err());
    assert!(CostThreshold::parse("-1s").is_err());
    assert!(CostThreshold::parse("-2GB").is_err());
    assert!(CostThreshold::parse("").is_err());
}

#[test]
fn threshold_selection_is_inclusive() {
    let at_bound = record("at_bound", 0, 0.5, 0);
    let below = record("below", 1, 0.49, 0);
    let t = CostThreshold::WallTimeSecs(0.5);
    assert!(t.is_met_by(&at_bound), "\u{2265} is inclusive at the bound");
    assert!(!t.is_met_by(&below));
}

#[test]
fn alloc_threshold_compares_alloc_not_time() {
    let slow_but_lean = record("slow_but_lean", 0, 100.0, MIB);
    let fast_but_fat = record("fast_but_fat", 1, 0.1, 9 * GIB);
    let t = CostThreshold::AllocBytes(8 * GIB);
    assert!(!t.is_met_by(&slow_but_lean));
    assert!(t.is_met_by(&fast_but_fat));
}

#[test]
fn describe_round_trips_units() {
    assert_eq!(CostThreshold::WallTimeSecs(0.5).describe(), "0.5s");
    assert_eq!(CostThreshold::AllocBytes(8 * GIB).describe(), "8.0GB");
}

// ── format_bytes ──

#[test]
fn format_bytes_picks_readable_unit() {
    assert_eq!(format_bytes(0), "0B");
    assert_eq!(format_bytes(17), "17B");
    assert_eq!(format_bytes(3277), "3.2KB");
    assert_eq!(format_bytes(512 * MIB), "512.0MB");
    // The §2.2 census-line example: 6.4GB.
    assert_eq!(format_bytes((6.4 * GIB as f64) as u64), "6.4GB");
    assert_eq!(format_bytes(15 * GIB + 410 * MIB), "15.4GB");
}

// ── ranking ──

#[test]
fn default_ranking_is_time_descending() {
    let mut records = vec![
        record("mid", 1, 12.1, 6 * GIB),
        record("top", 0, 42.8, GIB),
        record("low", 2, 0.2, 9 * GIB),
    ];
    sort_ranked(&mut records, None);
    let names: Vec<&str> = records.iter().map(|r| r.unit_name.as_str()).collect();
    assert_eq!(names, ["top", "mid", "low"]);
}

#[test]
fn alloc_threshold_reranks_by_alloc() {
    // The §1 item-2 pair: near-equal times, 2.4x apart in memory.
    let mut records = vec![
        record("parse_postfix_loop", 0, 40.8, 6 * GIB + 410 * MIB),
        record("try_parse_named_record", 1, 42.8, 15 * GIB + 410 * MIB),
        record("parse_unary_op", 2, 12.1, 6 * GIB + 410 * MIB),
    ];
    sort_ranked(&mut records, Some(&CostThreshold::AllocBytes(GIB)));
    let names: Vec<&str> = records.iter().map(|r| r.unit_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "try_parse_named_record",
            "parse_postfix_loop",
            "parse_unary_op"
        ],
        "alloc ranking separates what near-equal times hide; equal-alloc ties break by name"
    );
}

#[test]
fn equal_metric_ties_break_by_name_for_determinism() {
    let mut records = vec![record("zeta", 0, 1.0, 0), record("alpha", 1, 1.0, 0)];
    sort_ranked(&mut records, None);
    assert_eq!(records[0].unit_name, "alpha");
}

// ── gate verdict ──

#[test]
fn gate_passes_when_all_below_threshold() {
    let records = vec![record("a", 0, 0.1, MIB), record("b", 1, 0.4, MIB)];
    assert!(!gate_fails(&records, &CostThreshold::WallTimeSecs(0.5)));
}

#[test]
fn gate_fails_when_any_unit_meets_threshold() {
    let records = vec![record("a", 0, 0.1, MIB), record("b", 1, 0.6, MIB)];
    assert!(gate_fails(&records, &CostThreshold::WallTimeSecs(0.5)));
}

#[test]
fn gate_on_empty_census_passes() {
    assert!(!gate_fails(&[], &CostThreshold::WallTimeSecs(0.5)));
}

// ── rendering ──

#[test]
fn table_with_threshold_filters_and_summarizes() {
    let mut records = vec![
        record("heavy_one", 0, 60.5, 15 * GIB),
        record("heavy_two", 1, 39.3, 6 * GIB),
        record("light", 2, 0.1, MIB),
    ];
    sort_ranked(&mut records, None);
    let table = render_table(&records, Some(&CostThreshold::WallTimeSecs(0.5)));
    assert!(table.contains("heavy_one"));
    assert!(table.contains("heavy_two"));
    assert!(
        !table.contains("light"),
        "below-threshold rows are filtered"
    );
    assert!(
        table.contains("2 unit(s) \u{2265} 0.5s (of 3), 100s of 100s total"),
        "summary line missing or wrong: {table}"
    );
}

#[test]
fn empty_selection_summary_shows_positive_zero() {
    // f64's empty-iterator sum is -0.0; the summary must not print "-0s".
    let records = vec![record("light", 0, 0.001, 0)];
    let table = render_table(&records, Some(&CostThreshold::WallTimeSecs(0.5)));
    assert!(
        table.contains("0 unit(s) \u{2265} 0.5s (of 1), 0s of 0s total"),
        "summary must show 0s, not -0s: {table}"
    );
}

#[test]
fn empty_census_table_totals_show_positive_zero() {
    // With NO records at all, both sums are empty (-0.0); the summary must
    // still read 0s, not -0s.
    let table = render_table(&[], Some(&CostThreshold::WallTimeSecs(0.5)));
    assert!(
        table.contains("0 unit(s) \u{2265} 0.5s (of 0), 0s of 0s total"),
        "empty census summary must show 0s totals: {table}"
    );
}

#[test]
fn table_without_threshold_lists_every_unit() {
    let records = vec![record("a", 0, 1.0, MIB), record("b", 1, 0.01, MIB)];
    let table = render_table(&records, None);
    assert!(table.contains("a") && table.contains("b"));
    assert!(
        !table.contains("unit(s)"),
        "no summary line without a threshold"
    );
}

#[test]
fn json_carries_all_fields_and_totals() {
    let records = vec![
        record("elab__exprs__synth", 7, 60.5, 15 * GIB),
        record("parse_atom", 12, 39.25, 6 * GIB),
    ];
    let json = render_json(&records);
    assert!(json.contains("\"unit\":\"elab__exprs__synth\""));
    assert!(json.contains("\"index\":7"));
    assert!(json.contains("\"time_s\":60.500"));
    assert!(json.contains(&format!("\"alloc_bytes\":{}", 15 * GIB)));
    assert!(json.contains("\"total_time_s\":99.750"));
    assert!(json.contains(&format!("\"total_alloc_bytes\":{}", 21 * GIB)));
}

#[test]
fn empty_census_json_totals_show_positive_zero() {
    let json = render_json(&[]);
    assert!(
        json.contains("\"units\":[],\"total_time_s\":0.000,\"total_alloc_bytes\":0"),
        "empty census JSON must total 0.000, not -0.000: {json}"
    );
}

#[test]
fn json_escapes_hostile_unit_names() {
    let records = vec![record("we\"ird\\name", 0, 1.0, 0)];
    let json = render_json(&records);
    assert!(json.contains(r#""unit":"we\"ird\\name""#));
}

#[test]
fn json_escapes_control_chars_but_not_space() {
    // The control-char boundary is exclusive at 0x20: a newline (0x0A) must
    // become a six-character \u escape, while a space (0x20) must pass
    // through literally.
    let records = vec![record("line\nbreak and space", 0, 1.0, 0)];
    let json = render_json(&records);
    let expected = "\"unit\":\"line\\u000abreak and space\"";
    assert!(
        json.contains(expected),
        "newline must be \\u-escaped, space must stay literal: {json}"
    );
}

#[test]
fn serial_list_is_comma_separated_heaviest_first() {
    let mut records = vec![
        record("light", 0, 0.1, 0),
        record("second", 1, 12.1, 0),
        record("first", 2, 42.8, 0),
    ];
    sort_ranked(&mut records, None);
    assert_eq!(
        render_serial_list(&records, &CostThreshold::WallTimeSecs(0.5)),
        "first,second"
    );
}

#[test]
fn serial_list_empty_when_nothing_meets_threshold() {
    let records = vec![record("light", 0, 0.1, 0)];
    assert_eq!(
        render_serial_list(&records, &CostThreshold::WallTimeSecs(0.5)),
        ""
    );
}
