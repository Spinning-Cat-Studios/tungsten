//! JSON Lines protocol for sidecar IPC over Unix domain sockets.
//!
//! Each message is a single JSON object terminated by `\n`.
//! All messages include a `"v": 1` field for forward compatibility.
//! See ADR 21.4.26g §2.3 for the protocol specification.

use serde::{Deserialize, Serialize};

/// Protocol version for all messages.
pub const PROTOCOL_VERSION: u8 = 1;

// ═══════════════════════════════════════════════════════════════════════
// Incoming requests
// ═══════════════════════════════════════════════════════════════════════

/// A request from an agent to the sidecar process.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// Begin a debugging session with an error description.
    #[serde(rename = "start_session")]
    StartSession {
        error: String,
        #[serde(default)]
        file: Option<String>,
    },

    /// Request diagnostic tool suggestions for an error.
    #[serde(rename = "suggest")]
    Suggest { error: String },

    /// Report whether a diagnostic command helped.
    #[serde(rename = "report_outcome")]
    ReportOutcome { command: String, helped: bool },

    /// End the current session (or just disconnect).
    #[serde(rename = "end_session")]
    EndSession {
        #[serde(default)]
        summary: Option<String>,
    },

    /// Request a clean shutdown of the sidecar process.
    #[serde(rename = "shutdown")]
    Shutdown,
}

// ═══════════════════════════════════════════════════════════════════════
// Outgoing responses
// ═══════════════════════════════════════════════════════════════════════

/// A response from the sidecar process to an agent.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "session_started")]
    SessionStarted { v: u8, session_id: String },

    #[serde(rename = "suggestions")]
    Suggestions { v: u8, items: Vec<SuggestionItem> },

    #[serde(rename = "outcome_recorded")]
    OutcomeRecorded { v: u8 },

    #[serde(rename = "session_ended")]
    SessionEnded { v: u8 },

    #[serde(rename = "shutting_down")]
    ShuttingDown { v: u8 },

    #[serde(rename = "error")]
    Error { v: u8, message: String },
}

/// A suggestion item in the protocol response.
#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestionItem {
    pub command: String,
    pub cost: u8,
    pub relevance: f32,
    pub reason: String,
}

impl Response {
    pub fn session_started(session_id: String) -> Self {
        Self::SessionStarted {
            v: PROTOCOL_VERSION,
            session_id,
        }
    }

    pub fn suggestions(items: Vec<SuggestionItem>) -> Self {
        Self::Suggestions {
            v: PROTOCOL_VERSION,
            items,
        }
    }

    pub fn outcome_recorded() -> Self {
        Self::OutcomeRecorded {
            v: PROTOCOL_VERSION,
        }
    }

    pub fn session_ended() -> Self {
        Self::SessionEnded {
            v: PROTOCOL_VERSION,
        }
    }

    pub fn shutting_down() -> Self {
        Self::ShuttingDown {
            v: PROTOCOL_VERSION,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            v: PROTOCOL_VERSION,
            message: message.into(),
        }
    }
}
