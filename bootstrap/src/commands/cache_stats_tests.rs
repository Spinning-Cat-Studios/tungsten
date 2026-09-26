//! Tests for `cache status`'s rendering and outcome (ADR 5.8.26d P3).
//!
//! These assert the exact strings rather than "it printed something", because
//! the mutants that survived the first draft were all operators *inside* the
//! format arguments — `/`→`*` in the KB conversion, `>`→`<` on a size branch.
//! A test that only checks a line is present kills none of them.

use super::*;
use std::time::Duration;

fn stats(size_bytes: u64, entry_count: usize) -> CacheStats {
    CacheStats {
        size_bytes,
        entry_count,
        max_size_mb: 500,
        oldest_accessed: None,
        newest_accessed: None,
    }
}

fn elab(entry_count: usize, size_bytes: u64, full_count: usize, full_bytes: u64) -> ElabCacheStats {
    ElabCacheStats {
        entry_count,
        size_bytes,
        full_output_count: full_count,
        full_output_bytes: full_bytes,
    }
}

fn cwd_root() -> CacheRoot {
    cache_root::resolve(None, Path::new("/work/repo"))
}

// --- exit codes ------------------------------------------------------------

#[test]
fn a_failed_outcome_exits_nonzero_and_a_reported_one_succeeds() {
    // `ExitCode` is not comparable, so compare the debug rendering — the point
    // is that the two differ, which an all-SUCCESS command would not.
    let ok = format!("{:?}", CacheOutcome::Reported(vec![]).exit());
    let bad = format!("{:?}", CacheOutcome::Failed("x".into()).exit());
    assert_ne!(ok, bad, "a failure must not exit like a success");
    assert_eq!(ok, format!("{:?}", ExitCode::SUCCESS));
    assert_eq!(bad, format!("{:?}", ExitCode::from(3)));
}

#[test]
fn a_missing_operand_produces_a_failed_outcome() {
    let outcome = run_cache_stats(
        false,
        false,
        Some(Path::new("/definitely/not/here/main.tg")),
        Path::new("/work/repo"),
    );
    assert_eq!(
        outcome,
        CacheOutcome::Failed("no such file: /definitely/not/here/main.tg".to_string())
    );
}

#[test]
fn the_command_itself_exits_nonzero_on_a_missing_operand() {
    // Covers the CLI shell, not just `run_cache_stats`: without this, a mutant
    // replacing `cmd_cache_stats`'s return with `Default::default()` (== SUCCESS)
    // survives, silently turning a refusal into a success. Depends on no
    // checkout state — the operand is a path that cannot exist.
    let code = cmd_cache_stats(
        false,
        false,
        Some(Path::new("/definitely/not/here/main.tg")),
    );
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(3)));
}

// --- human rendering -------------------------------------------------------

#[test]
fn the_root_is_the_first_thing_reported_after_the_header() {
    // D5: a count must not be readable without the root it counts.
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), None);
    assert_eq!(lines[0], "Cache Statistics:");
    assert!(
        lines[1].starts_with("  Root:         /work/repo "),
        "{:?}",
        lines[1]
    );
    assert!(lines[1].contains("current directory"), "{:?}", lines[1]);
}

#[test]
fn an_entry_file_root_names_the_operand_it_came_from() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.tg");
    std::fs::write(&entry, "fn main() -> Nat { 0 }").unwrap();
    let root = cache_root::resolve(Some(&entry), Path::new("/work/repo"));

    let lines = format_stats_human(
        &root,
        Some(Path::new("src/compiler/main.tg")),
        &stats(0, 0),
        None,
    );

    assert!(
        lines[1].contains("(from src/compiler/main.tg)"),
        "{:?}",
        lines[1]
    );
    assert!(
        !lines[1].contains("/work/repo"),
        "cwd must not appear: {:?}",
        lines[1]
    );
}

#[test]
fn a_sub_megabyte_cache_reports_kb_only() {
    // 2048 bytes = 2 KB, 0 MB — exercises the `size_mb > 0` false branch and
    // pins the /1024 conversion.
    let lines = format_stats_human(&cwd_root(), None, &stats(2048, 7), None);
    assert!(
        lines.contains(&"  AST size:     2 KB".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"  AST entries:  7".to_string()),
        "{lines:?}"
    );
}

#[test]
fn a_multi_megabyte_cache_reports_both_mb_and_kb() {
    // 3 MB exactly: pins /(1024*1024) against /1024 and the `> 0` true branch.
    let lines = format_stats_human(&cwd_root(), None, &stats(3 * 1024 * 1024, 1), None);
    assert!(
        lines.contains(&"  AST size:     3 MB (3072 KB)".to_string()),
        "{lines:?}"
    );
}

#[test]
fn max_size_is_reported_in_mb() {
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), None);
    assert!(
        lines.contains(&"  Max size:     500 MB".to_string()),
        "{lines:?}"
    );
}

#[test]
fn without_elab_stats_no_elab_lines_appear() {
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), None);
    assert!(
        !lines.iter().any(|l| l.contains("Elab entries")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Full-output")),
        "{lines:?}"
    );
}

#[test]
fn elab_entries_and_size_are_reported_in_kb() {
    let e = elab(230, 8192, 0, 0);
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), Some(&e));
    assert!(
        lines.contains(&"  Elab entries: 230".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"  Elab size:    8 KB".to_string()),
        "{lines:?}"
    );
}

#[test]
fn a_zero_full_output_count_suppresses_the_full_output_line() {
    // The `full_output_count > 0` false branch — and the guard that keeps the
    // average below from dividing by zero.
    let e = elab(5, 1024, 0, 0);
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), Some(&e));
    assert!(
        !lines.iter().any(|l| l.contains("Full-output")),
        "{lines:?}"
    );
}

#[test]
fn the_full_output_line_reports_total_and_per_entry_average() {
    // 4 entries over 8192 bytes = 8 KB total, 2 KB/entry: pins both the /1024
    // and the /count divisions.
    let e = elab(4, 0, 4, 8192);
    let lines = format_stats_human(&cwd_root(), None, &stats(0, 0), Some(&e));
    let full = lines
        .iter()
        .find(|l| l.contains("Full-output"))
        .expect("full-output line missing");
    assert!(full.contains("4 entries"), "{full}");
    assert!(full.contains("8 KB"), "{full}");
    assert!(full.contains("avg 2 KB/entry"), "{full}");
}

#[test]
fn access_times_are_reported_only_when_present() {
    let mut s = stats(0, 0);
    let without = format_stats_human(&cwd_root(), None, &s, None);
    assert!(!without.iter().any(|l| l.contains("Oldest")), "{without:?}");

    s.oldest_accessed = Some(Duration::from_secs(0));
    s.newest_accessed = Some(Duration::from_secs(0));
    let with = format_stats_human(&cwd_root(), None, &s, None);
    assert!(with.iter().any(|l| l.contains("Oldest:")), "{with:?}");
    assert!(with.iter().any(|l| l.contains("Newest:")), "{with:?}");
}

// --- JSON rendering --------------------------------------------------------

#[test]
fn json_leads_with_the_root_and_carries_every_count() {
    let s = CacheStats {
        size_bytes: 4096,
        entry_count: 12,
        max_size_mb: 500,
        oldest_accessed: Some(Duration::from_millis(1500)),
        newest_accessed: Some(Duration::from_millis(2500)),
    };
    let e = elab(230, 8192, 230, 160_000_000);

    let json = format_stats_json(Path::new("/work/repo/src/compiler"), &s, Some(&e));

    assert_eq!(
        json,
        r#"{"root":"/work/repo/src/compiler","size_bytes":4096,"entry_count":12,"max_size_mb":500,"oldest_accessed_ms":1500,"newest_accessed_ms":2500,"elab_entry_count":230,"elab_size_bytes":8192}"#
    );
}

#[test]
fn json_absent_values_are_zero_not_omitted() {
    // Keys must stay present so a consumer can parse one shape unconditionally.
    let json = format_stats_json(Path::new("/w"), &stats(0, 0), None);
    assert!(json.contains(r#""oldest_accessed_ms":0"#), "{json}");
    assert!(json.contains(r#""elab_entry_count":0"#), "{json}");
    assert!(json.contains(r#""elab_size_bytes":0"#), "{json}");
}

#[test]
fn json_mode_returns_exactly_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = run_cache_stats(false, true, None, dir.path());
    match outcome {
        CacheOutcome::Reported(lines) => {
            assert_eq!(lines.len(), 1, "json must be one object: {lines:?}");
            assert!(lines[0].starts_with(r#"{"root":"#), "{:?}", lines[0]);
        }
        CacheOutcome::Failed(m) => panic!("unexpected failure: {m}"),
    }
}

#[test]
fn human_mode_reports_the_cwd_root_when_given_no_operand() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = run_cache_stats(false, false, None, dir.path());
    match outcome {
        CacheOutcome::Reported(lines) => {
            assert_eq!(lines[0], "Cache Statistics:");
            assert!(
                lines[1].contains(&dir.path().display().to_string()),
                "{:?}",
                lines[1]
            );
        }
        CacheOutcome::Failed(m) => panic!("unexpected failure: {m}"),
    }
}
