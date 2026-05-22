//! Integration tests for the sidecar process layer.
//!
//! These tests spawn a real server in a background thread, connect via
//! Unix domain socket, exchange JSON Lines messages, and verify LMDB state.
//! A static mutex serializes execution to avoid thread starvation.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

use crate::sidecar::store::ExperienceStore;

use super::integration_tests::{connect, send_and_recv, start_test_server, INTEGRATION_LOCK};

#[test]
fn test_unknown_request_type_returns_error() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // Valid JSON but unknown type
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"bogus_command","data":"test"}"#,
    );
    assert_eq!(resp["type"], "error");
    assert!(resp["message"]
        .as_str()
        .unwrap()
        .contains("invalid message"));

    // Server should still be alive
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    assert_eq!(resp["type"], "shutting_down");

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}

#[test]
fn test_missing_required_fields_returns_error() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // start_session without required "error" field
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"start_session"}"#,
    );
    assert_eq!(resp["type"], "error");

    // report_outcome without required "command" field
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","helped":true}"#,
    );
    assert_eq!(resp["type"], "error");

    // suggest without required "error" field
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"suggest"}"#);
    assert_eq!(resp["type"], "error");

    // Server should still be alive after all invalid messages
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    assert_eq!(resp["type"], "shutting_down");

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}

// ═══════════════════════════════════════════════════════════════════════
// Session lifecycle edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_session_replacement_flushes_first() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // Start first session and report an outcome
    send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"start_session","error":"first session"}"#,
    );
    send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","command":"cmd-first","helped":true}"#,
    );

    // Start second session WITHOUT ending the first — first should be flushed
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"start_session","error":"second session"}"#,
    );
    assert_eq!(resp["type"], "session_started");

    // Report outcome on second session
    send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","command":"cmd-second","helped":false}"#,
    );

    // End + shutdown
    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();

    // Both sessions should be persisted
    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.pattern_count, 2); // cmd-first and cmd-second
}

#[test]
fn test_suggest_without_session() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // Suggest without start_session — should work (stateless operation)
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"suggest","error":"type mismatch"}"#,
    );
    assert_eq!(resp["type"], "suggestions");
    assert!(resp["items"].is_array());

    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}

#[test]
fn test_suggest_empty_results() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // Suggest with a description that matches no patterns
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"suggest","error":"xyzzy completely unrelated nonsense"}"#,
    );
    assert_eq!(resp["type"], "suggestions");
    let items = resp["items"].as_array().unwrap();
    assert!(items.len() <= 10); // bounded, not exploding

    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}

#[test]
fn test_end_session_without_start_is_harmless() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // End session without starting one — should not crash
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    assert_eq!(resp["type"], "session_ended");

    // Server should still be alive
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    assert_eq!(resp["type"], "shutting_down");

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();

    // No sessions should be persisted
    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
}

#[test]
fn test_rapid_outcomes_all_flushed() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"start_session","error":"rapid fire test"}"#,
    );

    // Send 30 outcomes in quick succession
    for i in 0..30 {
        let msg = format!(
            r#"{{"v":1,"type":"report_outcome","command":"cmd-{i}","helped":{}}}"#,
            if i % 2 == 0 { "true" } else { "false" }
        );
        let resp = send_and_recv(&mut stream, &mut reader, &msg);
        assert_eq!(resp["type"], "outcome_recorded", "failed on outcome {i}");
    }

    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();

    // All 30 outcomes should be persisted
    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.pattern_count, 30);
}

// ═══════════════════════════════════════════════════════════════════════
// Lifecycle / cleanup tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_socket_and_pid_cleanup() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let pid_path = dir.path().join("sidecar.pid");

    // Verify files exist while running
    assert!(socket_path.exists());
    assert!(pid_path.exists());

    // Shutdown
    {
        let (mut stream, mut reader) = connect(&socket_path);
        send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    }

    server_handle.join().unwrap();

    // Verify cleanup
    assert!(!socket_path.exists(), "socket file should be removed");
    assert!(!pid_path.exists(), "pid file should be removed");
}
