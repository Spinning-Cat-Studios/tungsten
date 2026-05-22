//! Agent sidecar experience store (ADR 21.4.26f) and process layer (ADR 21.4.26g).
//!
//! Records agent debugging sessions, tracks which diagnostic commands helped,
//! and tunes recommendation relevance over time. Backed by LMDB via `heed`.
//!
//! The optional process layer provides a long-running sidecar communicating
//! over Unix domain sockets for implicit session management.

#[cfg(unix)]
pub mod process;
mod relevance;
pub mod store;
#[cfg(test)]
mod tests;

pub use relevance::{adjust_relevance, RelevanceEntry, MAX_BOOST, MIN_SAMPLES};
pub use store::ExperienceStore;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

/// A recorded debugging session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub timestamp: String,
    pub error_description: String,
    pub outcomes: Vec<CommandOutcome>,
}

/// Outcome of running a diagnostic command during a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub command: String,
    pub helped: bool,
    pub cost: u8,
}

// ═══════════════════════════════════════════════════════════════════════
// CLI subcommands
// ═══════════════════════════════════════════════════════════════════════

#[derive(Subcommand)]
pub enum SidecarCommands {
    /// Record a new debugging session
    ///
    /// Creates a session entry in the experience store and returns
    /// a session ID for use with `report-outcome`.
    ///
    /// Examples:
    ///   tungsten sidecar record-session --error "SIGSEGV in constructor"
    RecordSession {
        /// Error description for this session
        #[arg(long)]
        error: String,
    },

    /// Report whether a diagnostic command helped
    ///
    /// Records the outcome of running a command during a session.
    /// Use `ok` if it helped diagnose the issue, `no` if not.
    ///
    /// Examples:
    ///   tungsten sidecar report-outcome --session <id> check fold-consistency ok
    ///   tungsten sidecar report-outcome --session <id> emit-llvm no
    ReportOutcome {
        /// Session ID (from record-session)
        #[arg(long)]
        session: String,

        /// Command name that was run
        command: String,

        /// Whether the command helped: `ok` or `no`
        outcome: String,
    },

    /// Show experience store statistics
    ///
    /// Displays session count, pattern count, and top adjusted commands.
    ///
    /// Examples:
    ///   tungsten sidecar stats
    ///   tungsten sidecar stats --json
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clear all stored experience data
    ///
    /// Removes all sessions and relevance counts. The store falls back
    /// to static registry weights.
    ///
    /// Examples:
    ///   tungsten sidecar reset
    Reset,

    /// Export full store contents as JSON
    ///
    /// Dumps all sessions and relevance counts for inspection.
    ///
    /// Examples:
    ///   tungsten sidecar export --json
    Export {
        /// Output format (currently only JSON is supported)
        #[arg(long)]
        json: bool,
    },

    /// Start the sidecar background process
    ///
    /// Launches a long-running sidecar that communicates over a Unix domain
    /// socket. Returns immediately if already running. Prints the socket path.
    ///
    /// Examples:
    ///   tungsten sidecar start
    ///   tungsten sidecar start --repo-root /path/to/repo
    Start {
        /// Repository root path (defaults to current directory)
        #[arg(long)]
        repo_root: Option<PathBuf>,
    },

    /// Stop the sidecar background process
    ///
    /// Sends a shutdown message to the running sidecar, which flushes
    /// pending writes and exits cleanly.
    ///
    /// Examples:
    ///   tungsten sidecar stop
    Stop,

    /// Run the sidecar server (internal, used by `start`)
    #[command(hide = true)]
    Serve {
        /// Store directory (passed by `start`)
        #[arg(long)]
        store_dir: PathBuf,
    },
}

mod commands;
pub use commands::cmd_sidecar;
