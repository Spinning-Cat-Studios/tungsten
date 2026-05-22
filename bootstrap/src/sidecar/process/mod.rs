//! Sidecar process server — UDS listener, connection management, lifecycle.
//!
//! Launched by `tungsten sidecar start` via the hidden `sidecar serve`
//! subcommand. Listens on a Unix domain socket, spawns a thread per
//! connection, and auto-exits after an idle timeout.
//!
//! See ADR 21.4.26g for design rationale.

mod handler;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod integration_tests_advanced;
pub mod protocol;
#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use handler::ConnectionHandler;

use crate::sidecar::store::ExperienceStore;

/// Idle timeout before the server auto-exits (5 minutes).
const IDLE_TIMEOUT_SECS: u64 = 300;

/// Poll interval for the non-blocking accept loop.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Socket filename within the store directory.
pub const SOCKET_FILENAME: &str = "sidecar.sock";

/// PID filename within the store directory.
pub const PID_FILENAME: &str = "sidecar.pid";

/// Run the sidecar server. Blocks until shutdown or idle timeout.
///
/// Binds a Unix domain socket at `<store_dir>/sidecar.sock`,
/// writes a PID file, and enters the accept loop.
pub fn run_server(store_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = store_dir.join(SOCKET_FILENAME);
    let pid_path = store_dir.join(PID_FILENAME);

    // Ensure store directory exists
    fs::create_dir_all(store_dir)?;

    // Write PID file
    fs::write(&pid_path, std::process::id().to_string())?;

    // Remove stale socket if present
    let _ = fs::remove_file(&socket_path);

    // Open LMDB store
    let store = Arc::new(Mutex::new(ExperienceStore::open(store_dir)?));

    // Bind listener
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let active_connections = Arc::new(AtomicUsize::new(0));
    let last_activity = Arc::new(AtomicU64::new(now_secs()));

    let mut handles: Vec<thread::JoinHandle<()>> = Vec::new();

    // Accept loop
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                active_connections.fetch_add(1, Ordering::Relaxed);
                last_activity.store(now_secs(), Ordering::Relaxed);

                let store = store.clone();
                let shutdown = shutdown.clone();
                let active = active_connections.clone();
                let last_act = last_activity.clone();

                let h = thread::spawn(move || {
                    let mut handler = ConnectionHandler::new(store, shutdown);
                    handler.handle(stream);
                    active.fetch_sub(1, Ordering::Relaxed);
                    last_act.store(now_secs(), Ordering::Relaxed);
                });
                handles.push(h);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Check idle timeout (only when no active connections)
                if active_connections.load(Ordering::Relaxed) == 0 {
                    let idle_secs =
                        now_secs().saturating_sub(last_activity.load(Ordering::Relaxed));
                    if idle_secs >= IDLE_TIMEOUT_SECS {
                        break;
                    }
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                thread::sleep(POLL_INTERVAL);
            }
        }
    }

    // Wait for handler threads to finish
    for h in handles {
        let _ = h.join();
    }

    // Cleanup
    let _ = fs::remove_file(&socket_path);
    let _ = fs::remove_file(&pid_path);

    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
