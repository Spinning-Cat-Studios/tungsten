//! Opt-in activation gate for the diagnostic sidecar (ADR 23.7.26e D1).
//!
//! The sidecar experience loop is dogfooded only by Tungsten itself, so it is
//! OFF by default and flips on with a single boolean. Precedence:
//!   1. `TUNGSTEN_SIDECAR_ENABLED` env var (`1`/`true` → on, `0`/`false` → off),
//!   2. else `tungsten_sidecar_enabled` in the nearest `.claude/hooks/config.toml`
//!      (walking up from the current directory),
//!   3. else OFF.
//!
//! A missing file/key, an unparseable file, or a non-boolean value all read as
//! OFF — a repo that never opts in gets no sidecar and no spurious failure. The
//! key is co-located in the hooks config (beside the other per-repo agent
//! toggles); the hooks `ConfigToml` has no `deny_unknown_fields`, so this key
//! parses-and-ignores there rather than being a new cross-crate dependency.

use std::path::{Path, PathBuf};

/// Env var that overrides the config-file toggle (in both directions).
const ENV_VAR: &str = "TUNGSTEN_SIDECAR_ENABLED";

/// The hooks config file, relative to a repo root (walked up from cwd).
const HOOKS_CONFIG_REL: &str = ".claude/hooks/config.toml";

/// The boolean key read from the hooks config.
const TOGGLE_KEY: &str = "tungsten_sidecar_enabled";

/// The session id printed by `record-session` when the sidecar is disabled.
/// A recognizable sentinel rather than a real UUID, so a caller that threads it
/// into `report-outcome` (a no-op when disabled) never crashes.
pub const DISABLED_SESSION_ID: &str = "sidecar-disabled";

/// Whether the diagnostic sidecar is activated for this invocation.
///
/// Reads the process environment and current directory; see the module docs
/// for precedence. Every failure mode collapses to `false`.
pub fn sidecar_enabled() -> bool {
    let env_override = std::env::var(ENV_VAR).ok();
    let cwd = std::env::current_dir().ok();
    enabled_from(env_override.as_deref(), cwd.as_deref())
}

/// Pure core of [`sidecar_enabled`], split out so the precedence is testable
/// without touching process-global env/cwd: the env override wins when it
/// parses to a bool, otherwise the nearest hooks config's toggle decides
/// (default `false`).
fn enabled_from(env_override: Option<&str>, start: Option<&Path>) -> bool {
    if let Some(raw) = env_override {
        if let Some(b) = parse_bool(raw) {
            return b;
        }
    }
    start
        .and_then(find_hooks_config)
        .and_then(|path| read_toggle(&path))
        .unwrap_or(false)
}

/// Parse a boolean-ish toggle string. Recognized truthy/falsy spellings return
/// `Some`; anything else returns `None` so the caller falls through to the
/// config file rather than silently forcing a value.
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

/// Walk up from `start` for the nearest `.claude/hooks/config.toml` (nearest
/// ancestor wins), mirroring the tco-allowlist / code-health config discovery.
fn find_hooks_config(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(HOOKS_CONFIG_REL);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Read the `tungsten_sidecar_enabled` boolean from a hooks config file.
/// Returns `None` when the file is unreadable, unparseable, the key is absent,
/// or its value is not a boolean.
fn read_toggle(path: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    value.get(TOGGLE_KEY)?.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Write `.claude/hooks/config.toml` under `root` with the given body.
    fn write_hooks_config(root: &Path, body: &str) {
        let dir = root.join(".claude").join("hooks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), body).unwrap();
    }

    #[test]
    fn parse_bool_truthy_and_falsy_spellings() {
        for s in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert_eq!(parse_bool(s), Some(true), "{s:?} should be true");
        }
        for s in ["0", "false", "FALSE", "no", "off", ""] {
            assert_eq!(parse_bool(s), Some(false), "{s:?} should be false");
        }
    }

    #[test]
    fn parse_bool_unrecognized_is_none() {
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_bool("2"), None);
    }

    #[test]
    fn env_override_wins_over_config() {
        let dir = TempDir::new().unwrap();
        // Config says OFF, env says ON → env wins.
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = false\n");
        assert!(enabled_from(Some("1"), Some(dir.path())));
        // Config says ON, env says OFF → env wins.
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = true\n");
        assert!(!enabled_from(Some("0"), Some(dir.path())));
    }

    #[test]
    fn unrecognized_env_falls_through_to_config() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = true\n");
        assert!(enabled_from(Some("garbage"), Some(dir.path())));
    }

    #[test]
    fn config_toggle_true_enables() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = true\n");
        assert!(enabled_from(None, Some(dir.path())));
    }

    #[test]
    fn config_toggle_false_disables() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = false\n");
        assert!(!enabled_from(None, Some(dir.path())));
    }

    #[test]
    fn absent_key_is_off() {
        let dir = TempDir::new().unwrap();
        // A real-shaped hooks config that simply lacks the toggle → OFF.
        write_hooks_config(dir.path(), "allowed_roots = [\"/tmp\"]\n");
        assert!(!enabled_from(None, Some(dir.path())));
    }

    #[test]
    fn non_boolean_value_is_off() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = \"yes\"\n");
        assert!(!enabled_from(None, Some(dir.path())));
    }

    #[test]
    fn missing_config_file_is_off() {
        let dir = TempDir::new().unwrap();
        assert!(!enabled_from(None, Some(dir.path())));
    }

    #[test]
    fn no_start_dir_is_off() {
        assert!(!enabled_from(None, None));
    }

    #[test]
    fn find_hooks_config_picks_nearest_ancestor() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = true\n");
        let nested = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();
        // Walking up from a deep subdir still finds the root config.
        assert!(enabled_from(None, Some(nested.as_path())));
    }

    #[test]
    fn read_toggle_reads_the_boolean() {
        let dir = TempDir::new().unwrap();
        write_hooks_config(dir.path(), "tungsten_sidecar_enabled = true\n");
        let path = find_hooks_config(dir.path()).expect("config found");
        assert_eq!(read_toggle(&path), Some(true));
    }
}
