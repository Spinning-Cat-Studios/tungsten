//! LMDB-backed experience store using `heed`.
//!
//! Opens (or creates) a per-project LMDB environment at
//! `~/.tungsten/sidecar/<repo-root-hash>/`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use heed::types::Str;
use heed::{Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::relevance::RelevanceEntry;
use super::{CommandOutcome, Session};

/// Store statistics for the `sidecar stats` command.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoreStats {
    pub session_count: usize,
    pub pattern_count: usize,
    /// Top commands by success rate: (command, success_rate_pct).
    pub top_commands: Vec<(String, f32)>,
}

/// Full export of all store contents.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoreExport {
    pub sessions: Vec<Session>,
    pub relevance_counts: HashMap<String, RelevanceEntry>,
}

/// An LMDB-backed experience store.
///
/// Databases:
/// - `sessions`: UUID → JSON-serialized `Session`
/// - `relevance`: "pattern\0command" → JSON-serialized `RelevanceEntry`
pub struct ExperienceStore {
    env: Env,
    sessions_db: Database<Str, Str>,
    relevance_db: Database<Str, Str>,
}

impl ExperienceStore {
    /// Open (or create) the default store based on the current working directory.
    pub fn open_default() -> Result<Self, Box<dyn std::error::Error>> {
        let path = default_store_dir()?;
        Self::open(&path)
    }

    /// Open (or create) a store at the given directory path.
    #[allow(unsafe_code)]
    pub fn open(dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        fs::create_dir_all(dir)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(10 * 1024 * 1024) // 10 MB
                .max_dbs(2)
                .open(dir)?
        };

        let mut wtxn = env.write_txn()?;
        let sessions_db = env.create_database(&mut wtxn, Some("sessions"))?;
        let relevance_db = env.create_database(&mut wtxn, Some("relevance"))?;
        wtxn.commit()?;

        Ok(Self {
            env,
            sessions_db,
            relevance_db,
        })
    }

    /// Record a new debugging session. Returns the generated session ID.
    pub fn record_session(
        &mut self,
        error_description: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session_id = Uuid::new_v4().to_string();
        let session = Session {
            session_id: session_id.clone(),
            timestamp: now_iso8601(),
            error_description: error_description.to_string(),
            outcomes: Vec::new(),
        };

        let value = serde_json::to_string(&session)?;
        let mut wtxn = self.env.write_txn()?;
        self.sessions_db.put(&mut wtxn, &session_id, &value)?;
        wtxn.commit()?;

        Ok(session_id)
    }

    /// Report whether a diagnostic command helped during a session.
    ///
    /// Appends the outcome to the session and updates relevance counts.
    pub fn report_outcome(
        &mut self,
        session_id: &str,
        command: &str,
        helped: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut wtxn = self.env.write_txn()?;

        // Update session record
        let existing = self
            .sessions_db
            .get(&wtxn, session_id)?
            .ok_or_else(|| format!("session not found: {session_id}"))?;
        let mut session: Session = serde_json::from_str(existing)?;
        session.outcomes.push(CommandOutcome {
            command: command.to_string(),
            helped,
            cost: 0,
        });
        let updated = serde_json::to_string(&session)?;
        self.sessions_db.put(&mut wtxn, session_id, &updated)?;

        // Update relevance counts using error_description as the pattern key
        let rkey = relevance_key(&session.error_description, command);
        let mut entry = self.get_relevance_entry(&wtxn, &rkey)?;
        entry.shown_count += 1;
        if helped {
            entry.helped_count += 1;
        }
        let rval = serde_json::to_string(&entry)?;
        self.relevance_db.put(&mut wtxn, &rkey, &rval)?;

        wtxn.commit()?;
        Ok(())
    }

    /// Look up the relevance entry for a (pattern, command) pair.
    ///
    /// Returns `None` if no data exists for this pair.
    pub fn get_relevance(
        &self,
        pattern: &str,
        command: &str,
    ) -> Result<Option<RelevanceEntry>, Box<dyn std::error::Error>> {
        let rtxn = self.env.read_txn()?;
        let rkey = relevance_key(pattern, command);
        match self.relevance_db.get(&rtxn, &rkey)? {
            Some(val) => Ok(Some(serde_json::from_str(val)?)),
            None => Ok(None),
        }
    }

    /// Collect all relevance entries for a given pattern (error category).
    pub fn get_relevance_for_pattern(
        &self,
        pattern: &str,
    ) -> Result<HashMap<String, RelevanceEntry>, Box<dyn std::error::Error>> {
        let rtxn = self.env.read_txn()?;
        let prefix = format!("{pattern}\0");
        let mut result = HashMap::new();

        let iter = self.relevance_db.iter(&rtxn)?;
        for entry in iter {
            let (key, val) = entry?;
            if let Some(cmd) = key.strip_prefix(&prefix) {
                let re: RelevanceEntry = serde_json::from_str(val)?;
                result.insert(cmd.to_string(), re);
            }
        }

        Ok(result)
    }

    /// Compute store statistics.
    pub fn stats(&self) -> Result<StoreStats, Box<dyn std::error::Error>> {
        let rtxn = self.env.read_txn()?;

        let session_count = self.sessions_db.iter(&rtxn)?.count();

        let mut command_stats: HashMap<String, (u32, u32)> = HashMap::new();
        let mut pattern_count = 0;
        for entry in self.relevance_db.iter(&rtxn)? {
            let (key, val) = entry?;
            pattern_count += 1;
            let re: RelevanceEntry = serde_json::from_str(val)?;
            if let Some(cmd) = key.split('\0').nth(1) {
                let e = command_stats.entry(cmd.to_string()).or_default();
                e.0 += re.shown_count;
                e.1 += re.helped_count;
            }
        }

        let mut top_commands: Vec<(String, f32)> = command_stats
            .into_iter()
            .filter(|(_, (shown, _))| *shown > 0)
            .map(|(cmd, (shown, helped))| {
                let rate = (helped as f32 / shown as f32) * 100.0;
                (cmd, rate)
            })
            .collect();
        top_commands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_commands.truncate(10);

        Ok(StoreStats {
            session_count,
            pattern_count,
            top_commands,
        })
    }

    /// Clear all data in the store.
    pub fn reset(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut wtxn = self.env.write_txn()?;
        self.sessions_db.clear(&mut wtxn)?;
        self.relevance_db.clear(&mut wtxn)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Export all store contents.
    pub fn export_all(&self) -> Result<StoreExport, Box<dyn std::error::Error>> {
        let rtxn = self.env.read_txn()?;

        let mut sessions = Vec::new();
        for entry in self.sessions_db.iter(&rtxn)? {
            let (_, val) = entry?;
            sessions.push(serde_json::from_str(val)?);
        }

        let mut relevance_counts = HashMap::new();
        for entry in self.relevance_db.iter(&rtxn)? {
            let (key, val) = entry?;
            relevance_counts.insert(key.to_string(), serde_json::from_str(val)?);
        }

        Ok(StoreExport {
            sessions,
            relevance_counts,
        })
    }

    /// Write a complete session with all outcomes in a single LMDB transaction.
    ///
    /// Used by the process layer to batch writes accumulated during a
    /// connection. Also updates relevance counts for each outcome.
    pub fn flush_session(&mut self, session: &Session) -> Result<(), Box<dyn std::error::Error>> {
        let mut wtxn = self.env.write_txn()?;

        let value = serde_json::to_string(session)?;
        self.sessions_db
            .put(&mut wtxn, &session.session_id, &value)?;

        for outcome in &session.outcomes {
            let rkey = relevance_key(&session.error_description, &outcome.command);
            let mut entry = self.get_relevance_entry(&wtxn, &rkey)?;
            entry.shown_count += 1;
            if outcome.helped {
                entry.helped_count += 1;
            }
            let rval = serde_json::to_string(&entry)?;
            self.relevance_db.put(&mut wtxn, &rkey, &rval)?;
        }

        wtxn.commit()?;
        Ok(())
    }

    /// Internal: get or default a relevance entry within an existing txn.
    fn get_relevance_entry(
        &self,
        txn: &heed::RoTxn,
        key: &str,
    ) -> Result<RelevanceEntry, Box<dyn std::error::Error>> {
        match self.relevance_db.get(txn, key)? {
            Some(val) => Ok(serde_json::from_str(val)?),
            None => Ok(RelevanceEntry::default()),
        }
    }
}

/// Build the composite relevance key: "pattern\0command".
fn relevance_key(pattern: &str, command: &str) -> String {
    format!("{pattern}\0{command}")
}

/// Compute the store directory for a given repository root.
pub fn store_dir_for_root(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = dirs_home()?;
    let hash = path_hash(root);
    Ok(home.join(".tungsten").join("sidecar").join(hash))
}

/// Compute the default store directory using the current working directory.
pub fn default_store_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    store_dir_for_root(&cwd)
}

/// Get the home directory.
fn dirs_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "HOME environment variable not set".into())
}

/// SHA-256 hash of a path, used for per-repo store namespacing.
fn path_hash(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // 16-char hex prefix is sufficient
}

/// Current time in ISO 8601 format (no external time crate needed).
pub(crate) fn now_iso8601() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s", duration.as_secs())
}

/// Minimal hex encoding (avoids pulling in hex crate).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
