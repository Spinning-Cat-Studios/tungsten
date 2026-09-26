//! The `tungsten sidecar` CLI surface — the subcommand enum clap derives from.
//!
//! Lives here rather than in [`super`] for two reasons. It is the *only* part
//! of the sidecar that needs `clap`, so keeping it out of `mod.rs` leaves that
//! file to module wiring and the store-independent helpers. And it is
//! store-backed to a one: every variant either reads or writes the LMDB store
//! or the socket process, so it is compiled out on `wasm32` alongside them
//! (ADR 28.7.26a §2.1) with a single gate at its `mod` declaration instead of
//! one per item.
//!
//! Its implementations are the [`commands`] child module, so the enum and its
//! dispatcher share this one gate.

use std::path::PathBuf;

use clap::Subcommand;

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
