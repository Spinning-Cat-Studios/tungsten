//! Sidecar subcommand implementations.

use std::path::PathBuf;
use std::process::ExitCode;

use super::store::ExperienceStore;
use super::{process, store};
use super::{CommandOutcome, Session, SidecarCommands};

/// Dispatch a sidecar subcommand.
pub fn cmd_sidecar(cmd: SidecarCommands) -> ExitCode {
    match cmd {
        SidecarCommands::RecordSession { error } => cmd_record_session(&error),
        SidecarCommands::ReportOutcome {
            session,
            command,
            outcome,
        } => cmd_report_outcome(&session, &command, &outcome),
        SidecarCommands::Stats { json } => cmd_stats(json),
        SidecarCommands::Reset => cmd_reset(),
        SidecarCommands::Export { json } => cmd_export(json),
        SidecarCommands::Start { repo_root } => cmd_start(repo_root.as_deref()),
        SidecarCommands::Stop => cmd_stop(),
        SidecarCommands::Serve { store_dir } => cmd_serve(&store_dir),
    }
}

fn cmd_record_session(error: &str) -> ExitCode {
    let mut store = match ExperienceStore::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open sidecar store: {e}");
            return ExitCode::FAILURE;
        }
    };

    match store.record_session(error) {
        Ok(session_id) => {
            println!("{session_id}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to record session: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_report_outcome(session_id: &str, command: &str, outcome: &str) -> ExitCode {
    let helped = match outcome {
        "ok" | "yes" | "true" => true,
        "no" | "false" => false,
        _ => {
            eprintln!("error: outcome must be 'ok' or 'no', got '{outcome}'");
            return ExitCode::FAILURE;
        }
    };

    let mut store = match ExperienceStore::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open sidecar store: {e}");
            return ExitCode::FAILURE;
        }
    };

    match store.report_outcome(session_id, command, helped) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: failed to report outcome: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_stats(json: bool) -> ExitCode {
    let store = match ExperienceStore::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open sidecar store: {e}");
            return ExitCode::FAILURE;
        }
    };

    let stats = match store.stats() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read stats: {e}");
            return ExitCode::FAILURE;
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
    } else {
        println!("Sidecar Experience Store");
        println!("════════════════════════");
        println!("  Sessions:  {}", stats.session_count);
        println!("  Patterns:  {}", stats.pattern_count);
        if !stats.top_commands.is_empty() {
            println!();
            println!("  Top adjusted commands:");
            for (cmd, rate) in &stats.top_commands {
                println!("    {cmd}: {rate:.0}% success rate");
            }
        }
    }

    ExitCode::SUCCESS
}

fn cmd_reset() -> ExitCode {
    let store = match ExperienceStore::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open sidecar store: {e}");
            return ExitCode::FAILURE;
        }
    };

    match store.reset() {
        Ok(()) => {
            println!("✓ Sidecar store cleared");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to reset store: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_export(json: bool) -> ExitCode {
    if !json {
        eprintln!("error: --json is required for export");
        return ExitCode::FAILURE;
    }

    let store = match ExperienceStore::open_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open sidecar store: {e}");
            return ExitCode::FAILURE;
        }
    };

    match store.export_all() {
        Ok(data) => {
            println!("{}", serde_json::to_string_pretty(&data).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to export store: {e}");
            ExitCode::FAILURE
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Process lifecycle commands (ADR 21.4.26g)
// ═══════════════════════════════════════════════════════════════════════

fn cmd_start(repo_root: Option<&std::path::Path>) -> ExitCode {
    #[cfg(not(unix))]
    {
        let _ = repo_root;
        eprintln!("error: sidecar process requires Unix (macOS/Linux)");
        return ExitCode::FAILURE;
    }

    #[cfg(unix)]
    {
        let store_dir = match repo_root {
            Some(root) => store::store_dir_for_root(root),
            None => store::default_store_dir(),
        };
        let store_dir = match store_dir {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        let socket_path = store_dir.join(process::SOCKET_FILENAME);

        // Check if already running
        if socket_path.exists() && is_socket_alive(&socket_path) {
            println!("{}", socket_path.display());
            return ExitCode::SUCCESS;
        }

        // Clean stale files
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(store_dir.join(process::PID_FILENAME));

        // Spawn serve subprocess
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("error: cannot find own executable: {e}");
                return ExitCode::FAILURE;
            }
        };

        let _child = match std::process::Command::new(exe)
            .arg("sidecar")
            .arg("serve")
            .arg("--store-dir")
            .arg(&store_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: failed to spawn sidecar: {e}");
                return ExitCode::FAILURE;
            }
        };

        // Wait for socket to appear (up to 5 seconds)
        for _ in 0..50 {
            if socket_path.exists() && is_socket_alive(&socket_path) {
                println!("{}", socket_path.display());
                return ExitCode::SUCCESS;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        eprintln!("error: sidecar failed to start within 5 seconds");
        ExitCode::FAILURE
    }
}

fn cmd_stop() -> ExitCode {
    #[cfg(not(unix))]
    {
        eprintln!("error: sidecar process requires Unix (macOS/Linux)");
        return ExitCode::FAILURE;
    }

    #[cfg(unix)]
    {
        let store_dir = match store::default_store_dir() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        let socket_path = store_dir.join(process::SOCKET_FILENAME);

        if !socket_path.exists() {
            println!("sidecar is not running");
            return ExitCode::SUCCESS;
        }

        match send_shutdown(&socket_path) {
            Ok(()) => {
                // Wait for cleanup
                for _ in 0..30 {
                    if !socket_path.exists() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                println!("✓ Sidecar stopped");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to stop sidecar: {e}");
                // Force cleanup of stale files
                let _ = std::fs::remove_file(&socket_path);
                let _ = std::fs::remove_file(store_dir.join(process::PID_FILENAME));
                ExitCode::FAILURE
            }
        }
    }
}

fn cmd_serve(store_dir: &std::path::Path) -> ExitCode {
    #[cfg(not(unix))]
    {
        let _ = store_dir;
        eprintln!("error: sidecar process requires Unix (macOS/Linux)");
        return ExitCode::FAILURE;
    }

    #[cfg(unix)]
    {
        match process::run_server(store_dir) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: sidecar server failed: {e}");
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(unix)]
fn is_socket_alive(socket_path: &std::path::Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket_path).is_ok()
}

#[cfg(unix)]
fn send_shutdown(socket_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    writeln!(stream, r#"{{"v":1,"type":"shutdown"}}"#)?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    Ok(())
}
