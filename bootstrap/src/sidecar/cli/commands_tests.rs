//! Tests for the sidecar CLI opt-in gate policy (ADR 23.7.26e D1).
//!
//! `gate_decision` is pure — it takes the enabled flag explicitly — so the
//! whole "which commands no-op when the sidecar is off" policy is asserted here
//! without touching process-global env/cwd or the on-disk store.

use std::sync::Mutex;

use super::{cmd_sidecar, gate_decision, GateDecision, SidecarCommands};

/// Serializes the tests that mutate process-global env (`TUNGSTEN_SIDECAR_ENABLED`,
/// `HOME`) so they don't race each other (mirrors the `REGISTRY_GUARD` pattern).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Set or clear an env var back to a saved value.
fn restore_var(name: &str, saved: Option<String>) {
    match saved {
        Some(v) => std::env::set_var(name, v),
        None => std::env::remove_var(name),
    }
}

/// A `record-session` subcommand value for gate tests.
fn record() -> SidecarCommands {
    SidecarCommands::RecordSession {
        error: "boom".to_string(),
    }
}

/// A `report-outcome` subcommand value for gate tests.
fn report() -> SidecarCommands {
    SidecarCommands::ReportOutcome {
        session: "id".to_string(),
        command: "cmd".to_string(),
        outcome: "ok".to_string(),
    }
}

#[test]
fn enabled_always_runs_normally() {
    // Every command runs normally when the sidecar is on — including the gated
    // ones.
    for cmd in [
        record(),
        report(),
        SidecarCommands::Start { repo_root: None },
        SidecarCommands::Serve {
            store_dir: "/tmp/x".into(),
        },
        SidecarCommands::Stats { json: false },
        SidecarCommands::Reset,
        SidecarCommands::Export { json: true },
        SidecarCommands::Stop,
    ] {
        assert_eq!(gate_decision(&cmd, true), GateDecision::RunNormally);
    }
}

#[test]
fn disabled_record_session_prints_sentinel() {
    assert_eq!(
        gate_decision(&record(), false),
        GateDecision::NoOpPrintSentinel
    );
}

#[test]
fn disabled_store_and_socket_commands_noop() {
    // report-outcome / start / serve are the store-writing / socket-lifecycle
    // commands — they no-op (exit 0) when the sidecar is off.
    for cmd in [
        report(),
        SidecarCommands::Start { repo_root: None },
        SidecarCommands::Serve {
            store_dir: "/tmp/x".into(),
        },
    ] {
        assert_eq!(gate_decision(&cmd, false), GateDecision::NoOp);
    }
}

#[test]
fn disabled_read_and_manage_commands_still_run() {
    // stats/reset/export/stop only read or manage existing state — harmless with
    // the loop off, so they are NOT gated.
    for cmd in [
        SidecarCommands::Stats { json: false },
        SidecarCommands::Reset,
        SidecarCommands::Export { json: true },
        SidecarCommands::Stop,
    ] {
        assert_eq!(gate_decision(&cmd, false), GateDecision::RunNormally);
    }
}

#[test]
fn restore_var_sets_or_clears() {
    // Asserts the `restore_var` helper's contract (a mutated no-op would leave
    // env unchanged). Uses a dedicated key so it never perturbs the real toggle.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = "TUNGSTEN_SIDECAR_TEST_RESTORE";
    std::env::remove_var(key);
    restore_var(key, Some("val".to_string()));
    assert_eq!(std::env::var(key).ok(), Some("val".to_string()));
    restore_var(key, None);
    assert_eq!(std::env::var(key).ok(), None);
}

#[test]
fn sidecar_enabled_honours_env_override() {
    // The env override wins in both directions, deterministically (independent
    // of cwd/config) — pins `sidecar_enabled()`'s bool result.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = std::env::var("TUNGSTEN_SIDECAR_ENABLED").ok();

    std::env::set_var("TUNGSTEN_SIDECAR_ENABLED", "1");
    let on = crate::sidecar::sidecar_enabled();
    std::env::set_var("TUNGSTEN_SIDECAR_ENABLED", "0");
    let off = crate::sidecar::sidecar_enabled();

    restore_var("TUNGSTEN_SIDECAR_ENABLED", saved);
    assert!(on, "TUNGSTEN_SIDECAR_ENABLED=1 must enable");
    assert!(!off, "TUNGSTEN_SIDECAR_ENABLED=0 must disable");
}

#[test]
fn cmd_sidecar_enabled_dispatch_writes_a_session() {
    // When enabled, `cmd_sidecar(record-session)` must actually route to the
    // worker and persist a session (a stubbed always-SUCCESS dispatcher writes
    // nothing). HOME is redirected to a temp dir so the real store is untouched.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved_enabled = std::env::var("TUNGSTEN_SIDECAR_ENABLED").ok();
    let saved_home = std::env::var("HOME").ok();
    let home = tempfile::Builder::new()
        .prefix("tungsten-store-test-")
        .tempdir()
        .unwrap();

    std::env::set_var("HOME", home.path());
    std::env::set_var("TUNGSTEN_SIDECAR_ENABLED", "1");

    let _ = cmd_sidecar(SidecarCommands::RecordSession {
        error: "boom".to_string(),
    });

    // Read back from the store the dispatch wrote to (same HOME + cwd hash).
    let sessions = crate::sidecar::store::default_store_dir()
        .and_then(|dir| crate::sidecar::store::ExperienceStore::open(&dir))
        .and_then(|store| store.export_all())
        .map(|export| export.sessions)
        .unwrap_or_default();

    restore_var("TUNGSTEN_SIDECAR_ENABLED", saved_enabled);
    restore_var("HOME", saved_home);

    assert_eq!(
        sessions.len(),
        1,
        "enabled dispatch must persist one session"
    );
    assert_eq!(sessions[0].error_description, "boom");
}
