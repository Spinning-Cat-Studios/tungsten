//! Integration tests for the sidecar process layer.
//!
//! These tests spawn a real server in a background thread, connect via
//! Unix domain socket, exchange JSON Lines messages, and verify LMDB state.
//! A static mutex serializes execution to avoid thread starvation.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

use crate::sidecar::store::ExperienceStore;

/// Serialize integration tests that spawn servers to avoid thread starvation
/// under parallel test execution.
pub(super) static INTEGRATION_LOCK: Mutex<()> = Mutex::new(());

/// Send a JSON Lines message and read the response.
/// Retries on WouldBlock/TimedOut for up to 5 seconds (macOS
/// can return WouldBlock instead of TimedOut for socket timeouts).
pub(super) fn send_and_recv(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    msg: &str,
) -> serde_json::Value {
    writeln!(stream, "{msg}").unwrap();
    stream.flush().unwrap();
    let mut response = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match reader.read_line(&mut response) {
            Ok(0) => panic!("unexpected EOF from server"),
            Ok(_) => break,
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if std::time::Instant::now() >= deadline {
                    panic!("timed out waiting for server response");
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => panic!("read error: {e}"),
        }
    }
    serde_json::from_str(&response).unwrap()
}

/// Start a server in a background thread, wait for socket, return handles.
pub(super) fn start_test_server(
    store_dir: &std::path::Path,
) -> (std::path::PathBuf, thread::JoinHandle<()>) {
    let socket_path = store_dir.join("sidecar.sock");
    let sd = store_dir.to_path_buf();
    let server_handle = thread::spawn(move || {
        super::run_server(&sd).unwrap();
    });

    for _ in 0..100 {
        if socket_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(socket_path.exists(), "socket did not appear");

    (socket_path, server_handle)
}

/// Connect to the server and return stream + reader.
pub(super) fn connect(socket_path: &std::path::Path) -> (UnixStream, BufReader<UnixStream>) {
    let stream = UnixStream::connect(socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let reader = BufReader::new(stream.try_clone().unwrap());
    (stream, reader)
}

// ═══════════════════════════════════════════════════════════════════════
// Core workflow tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_full_workflow_via_socket() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    // 1. Start session
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"start_session","error":"SIGSEGV in test"}"#,
    );
    assert_eq!(resp["type"], "session_started");
    let session_id = resp["session_id"].as_str().unwrap();
    assert_eq!(session_id.len(), 36); // UUID format

    // 2. Suggest (stateless, doesn't need session)
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"suggest","error":"SIGSEGV"}"#,
    );
    assert_eq!(resp["type"], "suggestions");
    let items = resp["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(items[0]["command"]
        .as_str()
        .unwrap()
        .contains("check fold-consistency"));

    // 3. Report outcomes
    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","command":"check fold-consistency","helped":true}"#,
    );
    assert_eq!(resp["type"], "outcome_recorded");

    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","command":"emit-llvm","helped":false}"#,
    );
    assert_eq!(resp["type"], "outcome_recorded");

    // 4. End session
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    assert_eq!(resp["type"], "session_ended");

    // 5. Shutdown
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    assert_eq!(resp["type"], "shutting_down");

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();

    // 6. Verify data was flushed to LMDB
    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.pattern_count, 2);
}

#[test]
fn test_disconnect_flushes_session() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());

    // Connect, start session, report outcome, then disconnect without end_session
    {
        let (mut stream, mut reader) = connect(&socket_path);
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"start_session","error":"test disconnect"}"#,
        );
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"report_outcome","command":"some-cmd","helped":true}"#,
        );
        // Drop connection without end_session — should still flush
    }

    // Give the handler thread time to flush
    thread::sleep(Duration::from_millis(500));

    // Send shutdown via a new connection
    {
        let (mut stream, mut reader) = connect(&socket_path);
        send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    }

    server_handle.join().unwrap();

    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
}

#[test]
fn test_multiple_concurrent_connections() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());

    let sp1 = socket_path.clone();
    let client1 = thread::spawn(move || {
        let (mut stream, mut reader) = connect(&sp1);
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"start_session","error":"client 1 error"}"#,
        );
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"report_outcome","command":"cmd-a","helped":true}"#,
        );
        send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    });

    let sp2 = socket_path.clone();
    let client2 = thread::spawn(move || {
        let (mut stream, mut reader) = connect(&sp2);
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"start_session","error":"client 2 error"}"#,
        );
        send_and_recv(
            &mut stream,
            &mut reader,
            r#"{"v":1,"type":"report_outcome","command":"cmd-b","helped":false}"#,
        );
        send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"end_session"}"#);
    });

    client1.join().unwrap();
    client2.join().unwrap();

    {
        let (mut stream, mut reader) = connect(&socket_path);
        send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    }

    server_handle.join().unwrap();

    let store = ExperienceStore::open(dir.path()).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
}

// ═══════════════════════════════════════════════════════════════════════
// Error handling tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_invalid_json_returns_error() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    let resp = send_and_recv(&mut stream, &mut reader, "not valid json");
    assert_eq!(resp["type"], "error");
    assert!(resp["message"]
        .as_str()
        .unwrap()
        .contains("invalid message"));

    // Server should still be alive after invalid input
    let resp = send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    assert_eq!(resp["type"], "shutting_down");

    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}

#[test]
fn test_report_without_session_returns_error() {
    let _guard = INTEGRATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let (socket_path, server_handle) = start_test_server(dir.path());
    let (mut stream, mut reader) = connect(&socket_path);

    let resp = send_and_recv(
        &mut stream,
        &mut reader,
        r#"{"v":1,"type":"report_outcome","command":"cmd","helped":true}"#,
    );
    assert_eq!(resp["type"], "error");
    assert!(resp["message"]
        .as_str()
        .unwrap()
        .contains("no active session"));

    send_and_recv(&mut stream, &mut reader, r#"{"v":1,"type":"shutdown"}"#);
    drop(stream);
    drop(reader);
    server_handle.join().unwrap();
}
