//! `tungsten cache inspect <file>` — per-module elaboration-cache inspection
//! (ADR 4.7.26d §2.1).
//!
//! Reports, for each module in the project, which cache tier is present
//! (full-output / signature-only / uncached), its cached def count, and — for
//! the selected mode — whether that entry would serve `CoreDef` bodies to a
//! `run`/`test`. A signature-only entry answers "No", the 4.7.26c hazard that
//! `cache status`'s aggregate counts could never surface.

use std::path::Path;
use std::process::ExitCode;

use tungsten_bootstrap::driver::{self, CacheEntryKind, ModuleCacheRow};

/// Which mode's body-hazard the inspection reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectMode {
    Run,
    Test,
    Check,
}

impl InspectMode {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "run" => Some(Self::Run),
            "test" => Some(Self::Test),
            "check" => Some(Self::Check),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Test => "test",
            Self::Check => "check",
        }
    }

    /// Would the cache tier `kind` serve bodies for *this* mode?
    ///
    /// `check` never inspects bodies, so every tier is sound for it (always
    /// `true`). `run`/`test` need `CoreDef` bodies, so a signature-only entry is
    /// the hazard.
    fn serves_bodies(self, kind: CacheEntryKind) -> bool {
        match self {
            Self::Check => true,
            Self::Run | Self::Test => kind.serves_bodies(),
        }
    }
}

/// Entry point for `tungsten cache inspect <file> [--mode M] [--json]`.
pub fn cmd_cache_inspect(file: &Path, mode: &str, json: bool, verbose: bool) -> ExitCode {
    let Some(mode) = InspectMode::parse(mode) else {
        eprintln!("error: invalid --mode '{mode}' (expected run|test|check)");
        return ExitCode::from(2);
    };

    if !file.exists() {
        eprintln!("error: source file not found: {}", file.display());
        return ExitCode::from(3);
    }

    let rows = match driver::inspect_cache(file, verbose) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(3);
        }
    };

    if json {
        print!("{}", render_json(&rows, mode));
    } else {
        print!("{}", render_table(&rows, mode));
    }
    ExitCode::SUCCESS
}

/// Render the human-readable table (pure + unit-testable).
fn render_table(rows: &[ModuleCacheRow], mode: InspectMode) -> String {
    use std::fmt::Write as _;
    let bodies_col = format!("{}: bodies?", mode.label());
    let name_w = rows
        .iter()
        .map(|r| r.module.len())
        .max()
        .unwrap_or(0)
        .max("module".len());

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<name_w$}  {:<15}  {:>4}  {:<8}  {bodies_col}",
        "module", "entry", "defs", "hash",
    );
    let _ = writeln!(
        out,
        "{}  {}  {}  {}  {}",
        "─".repeat(name_w),
        "─".repeat(15),
        "─".repeat(4),
        "─".repeat(8),
        "─".repeat(bodies_col.len()),
    );
    for r in rows {
        let defs = r
            .def_count
            .map_or_else(|| "–".to_string(), |n| n.to_string());
        let serves = mode.serves_bodies(r.kind);
        let mut bodies = if serves {
            "yes".to_string()
        } else {
            "NO".to_string()
        };
        if !serves {
            bodies.push_str("  ← hazard for run/test (ADR 4.7.26c)");
        } else if r.kind == CacheEntryKind::Uncached {
            bodies.push_str(" (fresh elab)");
        }
        let _ = writeln!(
            out,
            "{:<name_w$}  {:<15}  {:>4}  {:<8}  {bodies}",
            r.module,
            r.kind.label(),
            defs,
            r.hash_prefix,
        );
    }
    out
}

/// Render the `--json` array (pure + unit-testable). Hand-rolled to avoid a
/// serde dependency for four scalar fields per row.
fn render_json(rows: &[ModuleCacheRow], mode: InspectMode) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("[");
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let defs = r
            .def_count
            .map_or_else(|| "null".to_string(), |n| n.to_string());
        let _ = write!(
            out,
            r#"{{"module":"{}","entry_kind":"{}","def_count":{},"serves_bodies":{},"hash":"{}"}}"#,
            r.module,
            r.kind.label(),
            defs,
            mode.serves_bodies(r.kind),
            r.hash_prefix,
        );
    }
    out.push_str("]\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(module: &str, kind: CacheEntryKind, def_count: Option<usize>) -> ModuleCacheRow {
        ModuleCacheRow {
            module: module.to_string(),
            kind,
            def_count,
            hash_prefix: "5f93b345".to_string(),
        }
    }

    #[test]
    fn run_mode_signature_only_is_hazard() {
        assert!(!InspectMode::Run.serves_bodies(CacheEntryKind::SignatureOnly));
        assert!(InspectMode::Run.serves_bodies(CacheEntryKind::FullOutput));
        assert!(InspectMode::Run.serves_bodies(CacheEntryKind::Uncached));
    }

    #[test]
    fn check_mode_never_hazardous() {
        for kind in [
            CacheEntryKind::SignatureOnly,
            CacheEntryKind::FullOutput,
            CacheEntryKind::Uncached,
        ] {
            assert!(InspectMode::Check.serves_bodies(kind));
        }
    }

    #[test]
    fn json_marks_signature_hazard() {
        let rows = vec![row(
            "compiler/main",
            CacheEntryKind::SignatureOnly,
            Some(10),
        )];
        let json = render_json(&rows, InspectMode::Run);
        assert!(json.contains(r#""entry_kind":"signature-only""#));
        assert!(json.contains(r#""def_count":10"#));
        assert!(json.contains(r#""serves_bodies":false"#));
    }

    #[test]
    fn json_uncached_serves_bodies() {
        let rows = vec![row("compiler/parser", CacheEntryKind::Uncached, None)];
        let json = render_json(&rows, InspectMode::Run);
        assert!(json.contains(r#""entry_kind":"(uncached)""#));
        assert!(json.contains(r#""def_count":null"#));
        assert!(json.contains(r#""serves_bodies":true"#));
    }

    #[test]
    fn table_flags_hazard_row() {
        let rows = vec![row(
            "compiler/main",
            CacheEntryKind::SignatureOnly,
            Some(10),
        )];
        let table = render_table(&rows, InspectMode::Run);
        assert!(table.contains("signature-only"));
        assert!(table.contains("NO"));
        assert!(table.contains("hazard"));
        assert!(table.contains("run: bodies?"));
    }
}
