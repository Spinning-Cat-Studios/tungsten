//! Unit tests for the sidecar experience store.

use tempfile::TempDir;

use super::relevance::{adjust_relevance, RelevanceEntry, MAX_BOOST, MIN_SAMPLES};
use super::store::ExperienceStore;

// ═══════════════════════════════════════════════════════════════════════
// Relevance tuning tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_below_min_samples_returns_base() {
    let entry = RelevanceEntry {
        shown_count: 3,
        helped_count: 3,
    };
    assert_eq!(adjust_relevance(0.5, &entry), 0.5);
}

#[test]
fn test_at_min_samples_adjusts() {
    let entry = RelevanceEntry {
        shown_count: MIN_SAMPLES,
        helped_count: MIN_SAMPLES,
    };
    let adjusted = adjust_relevance(0.5, &entry);
    // 100% success → adjustment = (1.0 - 0.5) × 0.3 = 0.15
    assert!((adjusted - 0.65).abs() < 0.001);
}

#[test]
fn test_zero_success_rate() {
    let entry = RelevanceEntry {
        shown_count: MIN_SAMPLES,
        helped_count: 0,
    };
    let adjusted = adjust_relevance(0.5, &entry);
    // 0% success → adjustment = (0.0 - 0.5) × 0.3 = -0.15
    assert!((adjusted - 0.35).abs() < 0.001);
}

#[test]
fn test_fifty_percent_no_change() {
    let entry = RelevanceEntry {
        shown_count: 10,
        helped_count: 5,
    };
    let adjusted = adjust_relevance(0.7, &entry);
    // 50% success → adjustment = 0
    assert!((adjusted - 0.7).abs() < 0.001);
}

#[test]
fn test_clamp_upper_bound() {
    let entry = RelevanceEntry {
        shown_count: 10,
        helped_count: 10,
    };
    let adjusted = adjust_relevance(0.95, &entry);
    assert!((adjusted - 1.0).abs() < 0.001);
}

#[test]
fn test_clamp_lower_bound() {
    let entry = RelevanceEntry {
        shown_count: 10,
        helped_count: 0,
    };
    let adjusted = adjust_relevance(0.1, &entry);
    // adjustment = -0.15, so 0.1 - 0.15 = -0.05 → clamped to 0.1
    assert!((adjusted - 0.1).abs() < 0.001);
}

#[test]
fn test_max_boost_magnitude() {
    assert!((MAX_BOOST - 0.3).abs() < 0.001);
}

#[test]
fn test_default_entry_is_zero() {
    let entry = RelevanceEntry::default();
    assert_eq!(entry.shown_count, 0);
    assert_eq!(entry.helped_count, 0);
    assert!(entry.success_rate().is_none());
}

// ═══════════════════════════════════════════════════════════════════════
// Store tests (using tempdir)
// ═══════════════════════════════════════════════════════════════════════

fn open_temp_store() -> (ExperienceStore, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = ExperienceStore::open(dir.path()).unwrap();
    (store, dir)
}

#[test]
fn test_store_open_creates_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("nested").join("store");
    let _store = ExperienceStore::open(&sub).unwrap();
    assert!(sub.exists());
}

#[test]
fn test_record_session_returns_uuid() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("test error").unwrap();
    // UUID v4 format: 8-4-4-4-12
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
}

#[test]
fn test_report_outcome_ok() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("SIGSEGV").unwrap();
    store
        .report_outcome(&id, "check fold-consistency", true)
        .unwrap();
    store.report_outcome(&id, "emit-llvm", false).unwrap();

    // Check relevance counts
    let entry = store
        .get_relevance("SIGSEGV", "check fold-consistency")
        .unwrap()
        .unwrap();
    assert_eq!(entry.shown_count, 1);
    assert_eq!(entry.helped_count, 1);

    let entry = store
        .get_relevance("SIGSEGV", "emit-llvm")
        .unwrap()
        .unwrap();
    assert_eq!(entry.shown_count, 1);
    assert_eq!(entry.helped_count, 0);
}

#[test]
fn test_report_outcome_unknown_session() {
    let (mut store, _dir) = open_temp_store();
    let result = store.report_outcome("nonexistent", "cmd", true);
    assert!(result.is_err());
}

#[test]
fn test_stats_empty_store() {
    let (store, _dir) = open_temp_store();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.pattern_count, 0);
    assert!(stats.top_commands.is_empty());
}

#[test]
fn test_stats_with_data() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("error A").unwrap();
    store.report_outcome(&id, "cmd1", true).unwrap();
    store.report_outcome(&id, "cmd2", false).unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.pattern_count, 2);
    assert_eq!(stats.top_commands.len(), 2);
}

#[test]
fn test_reset_clears_data() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("error").unwrap();
    store.report_outcome(&id, "cmd", true).unwrap();
    store.reset().unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.pattern_count, 0);
}

#[test]
fn test_export_empty() {
    let (store, _dir) = open_temp_store();
    let data = store.export_all().unwrap();
    assert!(data.sessions.is_empty());
    assert!(data.relevance_counts.is_empty());
}

#[test]
fn test_export_with_data() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("test").unwrap();
    store.report_outcome(&id, "cmd", true).unwrap();

    let data = store.export_all().unwrap();
    assert_eq!(data.sessions.len(), 1);
    assert_eq!(data.sessions[0].outcomes.len(), 1);
    assert!(data.sessions[0].outcomes[0].helped);
    assert!(!data.relevance_counts.is_empty());
}

#[test]
fn test_get_relevance_for_pattern() {
    let (mut store, _dir) = open_temp_store();
    let id1 = store.record_session("SIGSEGV").unwrap();
    store.report_outcome(&id1, "check-fold", true).unwrap();
    store.report_outcome(&id1, "emit-llvm", false).unwrap();

    let id2 = store.record_session("type mismatch").unwrap();
    store.report_outcome(&id2, "trace-types", true).unwrap();

    let sig_entries = store.get_relevance_for_pattern("SIGSEGV").unwrap();
    assert_eq!(sig_entries.len(), 2);
    assert!(sig_entries.contains_key("check-fold"));
    assert!(sig_entries.contains_key("emit-llvm"));

    let type_entries = store.get_relevance_for_pattern("type mismatch").unwrap();
    assert_eq!(type_entries.len(), 1);
}

#[test]
fn test_cumulative_counts() {
    let (mut store, _dir) = open_temp_store();
    let id1 = store.record_session("error").unwrap();
    store.report_outcome(&id1, "cmd", true).unwrap();

    let id2 = store.record_session("error").unwrap();
    store.report_outcome(&id2, "cmd", false).unwrap();

    let entry = store.get_relevance("error", "cmd").unwrap().unwrap();
    assert_eq!(entry.shown_count, 2);
    assert_eq!(entry.helped_count, 1);
}

// ═══════════════════════════════════════════════════════════════════════
// Additional coverage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_success_rate_exactly_at_min_samples() {
    let entry = RelevanceEntry {
        shown_count: MIN_SAMPLES,
        helped_count: 2,
    };
    let rate = entry
        .success_rate()
        .expect("should return Some at MIN_SAMPLES");
    assert!((rate - 0.4).abs() < 0.001);
}

#[test]
fn test_success_rate_below_min_samples_is_none() {
    let entry = RelevanceEntry {
        shown_count: MIN_SAMPLES - 1,
        helped_count: 2,
    };
    assert!(entry.success_rate().is_none());
}

#[test]
fn test_get_relevance_missing_pair_returns_none() {
    let (store, _dir) = open_temp_store();
    let result = store.get_relevance("nonexistent", "cmd").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_relevance_for_pattern_empty_store() {
    let (store, _dir) = open_temp_store();
    let entries = store.get_relevance_for_pattern("anything").unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_stats_top_commands_sorted_by_success_rate() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("err").unwrap();
    // cmd_low: 0% success
    store.report_outcome(&id, "cmd_low", false).unwrap();
    // cmd_high: 100% success
    store.report_outcome(&id, "cmd_high", true).unwrap();

    let stats = store.stats().unwrap();
    assert!(stats.top_commands.len() >= 2);
    // First entry should have higher success rate
    assert!(stats.top_commands[0].1 >= stats.top_commands[1].1);
    assert_eq!(stats.top_commands[0].0, "cmd_high");
}

#[test]
fn test_stats_top_commands_truncated_at_10() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("err").unwrap();
    for i in 0..15 {
        let cmd = format!("cmd_{i}");
        store.report_outcome(&id, &cmd, i % 2 == 0).unwrap();
    }

    let stats = store.stats().unwrap();
    assert!(stats.top_commands.len() <= 10);
}

#[test]
fn test_stats_aggregates_across_sessions() {
    let (mut store, _dir) = open_temp_store();
    // Two sessions report on the same command
    let id1 = store.record_session("err").unwrap();
    store.report_outcome(&id1, "cmd", true).unwrap();

    let id2 = store.record_session("err").unwrap();
    store.report_outcome(&id2, "cmd", true).unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
    // One (pattern,command) pair, but shown_count=2
    assert_eq!(stats.top_commands.len(), 1);
    assert!((stats.top_commands[0].1 - 100.0).abs() < 0.001); // 2/2 = 100%
}

#[test]
fn test_export_json_round_trip() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("test round trip").unwrap();
    store.report_outcome(&id, "cmd_a", true).unwrap();
    store.report_outcome(&id, "cmd_b", false).unwrap();

    let data = store.export_all().unwrap();
    let json = serde_json::to_string(&data).unwrap();
    let parsed: super::store::StoreExport = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.sessions.len(), 1);
    assert_eq!(parsed.sessions[0].outcomes.len(), 2);
    assert!(!parsed.relevance_counts.is_empty());
}

#[test]
fn test_session_serialization_round_trip() {
    let session = super::Session {
        session_id: "test-id".to_string(),
        timestamp: "12345s".to_string(),
        error_description: "SIGSEGV".to_string(),
        outcomes: vec![
            super::CommandOutcome {
                command: "check-fold".to_string(),
                helped: true,
                cost: 3,
            },
            super::CommandOutcome {
                command: "emit-llvm".to_string(),
                helped: false,
                cost: 4,
            },
        ],
    };
    let json = serde_json::to_string(&session).unwrap();
    let restored: super::Session = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.session_id, "test-id");
    assert_eq!(restored.outcomes.len(), 2);
    assert!(restored.outcomes[0].helped);
    assert!(!restored.outcomes[1].helped);
}

#[test]
fn test_multiple_outcomes_per_session_preserved() {
    let (mut store, _dir) = open_temp_store();
    let id = store.record_session("multi-outcome test").unwrap();
    store.report_outcome(&id, "cmd1", true).unwrap();
    store.report_outcome(&id, "cmd2", false).unwrap();
    store.report_outcome(&id, "cmd3", true).unwrap();

    let data = store.export_all().unwrap();
    let session = &data.sessions[0];
    assert_eq!(session.outcomes.len(), 3);
    assert_eq!(session.outcomes[0].command, "cmd1");
    assert!(session.outcomes[0].helped);
    assert_eq!(session.outcomes[1].command, "cmd2");
    assert!(!session.outcomes[1].helped);
    assert_eq!(session.outcomes[2].command, "cmd3");
    assert!(session.outcomes[2].helped);
}
