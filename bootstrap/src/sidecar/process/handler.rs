//! Per-connection handler with implicit session state.
//!
//! Each connection to the sidecar process gets its own handler,
//! which manages a single session. Outcomes are accumulated in memory
//! and flushed to LMDB on disconnect or `end_session`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use uuid::Uuid;

use crate::doctor::suggest_tools::match_suggestions;
use crate::sidecar::store::{now_iso8601, ExperienceStore};
use crate::sidecar::{CommandOutcome, Session};

use super::protocol::{Request, Response, SuggestionItem};

/// Per-connection handler with implicit session state.
pub struct ConnectionHandler {
    session: Option<Session>,
    store: Arc<Mutex<ExperienceStore>>,
    shutdown: Arc<AtomicBool>,
}

impl ConnectionHandler {
    pub fn new(store: Arc<Mutex<ExperienceStore>>, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            session: None,
            store,
            shutdown,
        }
    }

    /// Handle a single client connection, reading JSON Lines until EOF
    /// or shutdown, then flushing any accumulated session data.
    pub fn handle(&mut self, stream: UnixStream) {
        // Ensure the accepted stream is blocking — the listener is non-blocking,
        // and on some platforms accepted connections may inherit that.
        stream.set_nonblocking(false).ok();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let write_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut reader = BufReader::new(stream);
        let mut writer = write_stream;
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF — client disconnected
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let response = self.dispatch(trimmed);
                    let is_shutdown = matches!(response, Response::ShuttingDown { .. });
                    let is_ended = matches!(response, Response::SessionEnded { .. });
                    if send_response(&mut writer, &response).is_err() {
                        break;
                    }
                    if is_shutdown || is_ended {
                        if is_shutdown {
                            break;
                        }
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if self.shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        self.flush();
    }

    fn dispatch(&mut self, line: &str) -> Response {
        let request: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => return Response::error(format!("invalid message: {e}")),
        };

        match request {
            Request::StartSession { error, .. } => {
                // Flush any prior session before starting a new one
                self.flush();
                let session_id = Uuid::new_v4().to_string();
                self.session = Some(Session {
                    session_id: session_id.clone(),
                    timestamp: now_iso8601(),
                    error_description: error,
                    outcomes: Vec::new(),
                });
                Response::session_started(session_id)
            }
            Request::Suggest { error } => {
                let scored = match_suggestions(&error);
                let items = scored
                    .into_iter()
                    .map(|s| SuggestionItem {
                        command: s.command.to_string(),
                        cost: s.cost,
                        relevance: s.relevance,
                        reason: s.reason.to_string(),
                    })
                    .collect();
                Response::suggestions(items)
            }
            Request::ReportOutcome { command, helped } => {
                if let Some(ref mut session) = self.session {
                    session.outcomes.push(CommandOutcome {
                        command,
                        helped,
                        cost: 0,
                    });
                    Response::outcome_recorded()
                } else {
                    Response::error("no active session; send start_session first")
                }
            }
            Request::EndSession { .. } => {
                self.flush();
                Response::session_ended()
            }
            Request::Shutdown => {
                self.shutdown.store(true, Ordering::Relaxed);
                self.flush();
                Response::shutting_down()
            }
        }
    }

    fn flush(&mut self) {
        if let Some(session) = self.session.take() {
            if let Ok(mut store) = self.store.lock() {
                let _ = store.flush_session(&session);
            }
        }
    }
}

fn send_response(writer: &mut UnixStream, response: &Response) -> std::io::Result<()> {
    let mut json = serde_json::to_string(response).unwrap_or_default();
    json.push('\n');
    writer.write_all(json.as_bytes())?;
    writer.flush()
}
