//! Agent sidecar experience store (ADR 21.4.26f) and process layer (ADR 21.4.26g).
//!
//! Records agent debugging sessions, tracks which diagnostic commands helped,
//! and tunes recommendation relevance over time. Backed by LMDB via `heed`.
//!
//! The optional process layer provides a long-running sidecar communicating
//! over Unix domain sockets for implicit session management.
//!
//! Everything that touches LMDB or spawns a process is compiled out on
//! `wasm32`, matching the target scope its `heed` and `uuid` dependencies carry
//! in `Cargo.toml` (ADR 28.7.26a §2.1). What stays unconditional is the part
//! with no such dependency and a caller that outlives the store:
//! [`sidecar_enabled`] (a config read) and [`adjust_relevance`] (arithmetic on
//! an already-loaded entry). `doctor suggest-tools` consults both, and keeping
//! them target-invariant is what lets it keep its static ranking rather than
//! disappearing along with the store.

mod config;
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub mod process;
mod relevance;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

pub use config::{sidecar_enabled, DISABLED_SESSION_ID};
pub use relevance::{adjust_relevance, RelevanceEntry, MAX_BOOST, MIN_SAMPLES};
#[cfg(not(target_arch = "wasm32"))]
pub use store::ExperienceStore;

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

// The store-backed surface: the CLI enum and its implementations, both
// compiled out wherever the store is (ADR 28.7.26a §2.1).
#[cfg(not(target_arch = "wasm32"))]
mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::{cmd_sidecar, SidecarCommands};
