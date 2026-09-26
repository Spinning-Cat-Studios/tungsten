//! Unit tests for the sidecar experience store.

// The SysV semaphore leak is a macOS-only condition: LMDB selects its SysV
// locking backend on Apple/BSD and POSIX mutexes elsewhere (ADR 18.8.26d
// D2a), so the reproduction would pass on Linux for the wrong reason.
#[cfg(target_os = "macos")]
mod semaphore_reclaim;

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

/// The directory is returned FIRST deliberately: tuple bindings drop in
/// reverse declaration order, so `let (_dir, store)` drops the store before
/// the directory — and the close in the store's `Drop` needs `lock.mdb` to
/// still exist to release the environment's SysV semaphore set
/// (ADR 18.8.26d §1.3).
fn open_temp_store() -> (TempDir, ExperienceStore) {
    let dir = tempfile::Builder::new()
        .prefix("tungsten-store-test-")
        .tempdir()
        .unwrap();
    let store = ExperienceStore::open(dir.path()).unwrap();
    (dir, store)
}

/// The teardown warning's polarity (ADR 18.8.26d): silence when the bounded
/// wait saw the environment close, a warning only when it did not.
#[test]
fn close_warning_fires_only_when_the_environment_stayed_open() {
    assert!(super::store::close_warning(true).is_none());
    let warning = super::store::close_warning(false).expect("timeout must warn");
    assert!(warning.contains("still open"));
}

#[test]
fn test_store_open_creates_dir() {
    let dir = tempfile::Builder::new()
        .prefix("tungsten-store-test-")
        .tempdir()
        .unwrap();
    let sub = dir.path().join("nested").join("store");
    let _store = ExperienceStore::open(&sub).unwrap();
    assert!(sub.exists());
}

#[test]
fn test_record_session_returns_uuid() {
    let (_dir, mut store) = open_temp_store();
    let id = store.record_session("test error").unwrap();
    // UUID v4 format: 8-4-4-4-12
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
}

#[test]
fn test_report_outcome_ok() {
    let (_dir, mut store) = open_temp_store();
    let id = store.record_session("SIGSEGV").unwrap();
    store
        .report_outcome(&id, "check fold-consistency", true)
        .unwrap();
    store.report_outcome(&id, "emit-llvm", false).unwrap();

    // Relevance is keyed by the normalized error *class* (ADR 23.7.26e D2):
    // "SIGSEGV" belongs to the "segfault" category, not its verbatim text.
    let entry = store
        .get_relevance("segfault", "check fold-consistency")
        .unwrap()
        .unwrap();
    assert_eq!(entry.shown_count, 1);
    assert_eq!(entry.helped_count, 1);

    let entry = store
        .get_relevance("segfault", "emit-llvm")
        .unwrap()
        .unwrap();
    assert_eq!(entry.shown_count, 1);
    assert_eq!(entry.helped_count, 0);

    // The verbatim description is no longer a key — that was the pre-fix bug.
    assert!(store
        .get_relevance("SIGSEGV", "check fold-consistency")
        .unwrap()
        .is_none());
}

#[test]
fn test_report_outcome_unknown_session() {
    let (_dir, mut store) = open_temp_store();
    let result = store.report_outcome("nonexistent", "cmd", true);
    assert!(result.is_err());
}

#[test]
fn test_stats_empty_store() {
    let (_dir, store) = open_temp_store();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.pattern_count, 0);
    assert!(stats.top_commands.is_empty());
}

#[test]
fn test_stats_with_data() {
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
    let id = store.record_session("error").unwrap();
    store.report_outcome(&id, "cmd", true).unwrap();
    store.reset().unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.pattern_count, 0);
}

#[test]
fn test_export_empty() {
    let (_dir, store) = open_temp_store();
    let data = store.export_all().unwrap();
    assert!(data.sessions.is_empty());
    assert!(data.relevance_counts.is_empty());
}

#[test]
fn test_export_with_data() {
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
    let id1 = store.record_session("SIGSEGV").unwrap();
    store.report_outcome(&id1, "check-fold", true).unwrap();
    store.report_outcome(&id1, "emit-llvm", false).unwrap();

    let id2 = store.record_session("type mismatch").unwrap();
    store.report_outcome(&id2, "trace-types", true).unwrap();

    // Entries are grouped under the normalized error class (ADR 23.7.26e D2):
    // "SIGSEGV" → "segfault", "type mismatch" → "type mismatch".
    let sig_entries = store.get_relevance_for_pattern("segfault").unwrap();
    assert_eq!(sig_entries.len(), 2);
    assert!(sig_entries.contains_key("check-fold"));
    assert!(sig_entries.contains_key("emit-llvm"));

    let type_entries = store.get_relevance_for_pattern("type mismatch").unwrap();
    assert_eq!(type_entries.len(), 1);
}

#[test]
fn test_learning_accumulates_across_verbatim_descriptions() {
    // AC2 (write side): different verbatim descriptions in the SAME error class
    // must accumulate under one normalized key, so learning actually fires on
    // recurring classes rather than requiring the exact free-text to recur.
    let (_dir, mut store) = open_temp_store();
    // Five distinct verbatim descriptions, each carrying a UNIQUE segfault
    // keyword and otherwise-neutral words (avoiding e.g. "constructor", whose
    // "ctor" substring would pull it into a different, higher-scoring class).
    let descriptions = [
        "sigsegv observed here",
        "a segfault occurred",
        "segmentation fault reported",
        "signal 11 raised",
        "null pointer encountered",
    ];
    for desc in descriptions {
        let id = store.record_session(desc).unwrap();
        store
            .report_outcome(&id, "tungsten doctor check fold-consistency <file>", true)
            .unwrap();
    }

    // All five verbatim descriptions collapse to the "segfault" class.
    let entries = store.get_relevance_for_pattern("segfault").unwrap();
    let entry = entries
        .get("tungsten doctor check fold-consistency <file>")
        .expect("command learned under the segfault class");
    assert_eq!(entry.shown_count, 5);
    assert_eq!(entry.helped_count, 5);
    // Reaches MIN_SAMPLES, so success_rate is now available for adjustment.
    assert_eq!(entry.success_rate(), Some(1.0));
}

#[test]
fn test_report_outcome_captures_cost_from_registry() {
    // AC2 (cost side): a known registry command records its cost tier, not 0.
    let (_dir, mut store) = open_temp_store();
    let id = store
        .record_session("wrong value printed by binary")
        .unwrap();
    store
        .report_outcome(&id, "tungsten diff exec <file>", true)
        .unwrap();

    let data = store.export_all().unwrap();
    let outcome = &data.sessions[0].outcomes[0];
    // `tungsten diff exec <file>` is a cost-5 suggestion in the registry.
    assert_eq!(outcome.cost, 5);
}

#[test]
fn test_report_outcome_unknown_command_cost_is_zero() {
    // A command not in the registry has no known cost — stays 0 (unknown).
    let (_dir, mut store) = open_temp_store();
    let id = store.record_session("SIGSEGV").unwrap();
    store
        .report_outcome(&id, "not-a-real-command", true)
        .unwrap();

    let data = store.export_all().unwrap();
    assert_eq!(data.sessions[0].outcomes[0].cost, 0);
}

#[test]
fn test_cumulative_counts() {
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, store) = open_temp_store();
    let result = store.get_relevance("nonexistent", "cmd").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_get_relevance_for_pattern_empty_store() {
    let (_dir, store) = open_temp_store();
    let entries = store.get_relevance_for_pattern("anything").unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_stats_top_commands_sorted_by_success_rate() {
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
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
    let (_dir, mut store) = open_temp_store();
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
