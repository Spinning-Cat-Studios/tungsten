//! Project-root resolution for the read-only `tungsten cache` commands (ADR 5.8.26d D5).
//!
//! `cache status` resolved its root from the CURRENT WORKING DIRECTORY, while
//! every writer resolves it from the ENTRY FILE's parent (`driver/cache.rs`
//! `prepare_project_with_cache`). So a status run from the repo root reported
//! `Elab entries: 0` while 726 entries sat under `src/compiler/.tungsten/` —
//! two commands disagreeing about which cache exists, with no way to tell from
//! the output which one had been consulted. `cache clean` walks for `.tungsten`
//! directories and was always right, which is what made the disagreement so
//! convincing in the wrong direction.
//!
//! Two properties close it: the root is ALWAYS reported (so `0 entries` about
//! an uninspected project is unrepresentable), and an optional path operand
//! resolves the root the way the writer does.

use std::path::{Path, PathBuf};

/// Where a resolved cache root came from — reported so a count is never
/// readable without the root it counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootSource {
    /// Derived from an entry-file operand, the way the writer derives it.
    EntryFile,
    /// The process's current working directory — no operand was given.
    Cwd,
}

/// A cache root plus the reason it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheRoot {
    /// The directory whose `.tungsten/` subtree will be inspected.
    pub(crate) path: PathBuf,
    /// How `path` was arrived at.
    pub(crate) source: RootSource,
}

impl CacheRoot {
    /// Human-readable provenance, e.g. `(from src/compiler/test_strmap.tg)`.
    pub(crate) fn provenance(&self, entry: Option<&Path>) -> String {
        match (self.source, entry) {
            (RootSource::EntryFile, Some(e)) => format!("(from {})", e.display()),
            // An `EntryFile` root always has an operand; the arm exists so a
            // future caller cannot make the label silently disagree with the source.
            (RootSource::EntryFile, None) => "(from entry file)".to_string(),
            (RootSource::Cwd, _) => {
                "(current directory — pass a file to inspect its project)".to_string()
            }
        }
    }
}

/// Reject an operand that names nothing, before any root is resolved.
///
/// Lives beside `resolve` rather than in any one command's module because every
/// `cache` command that takes an operand needs it — `status`, `clean` and
/// `prune` all call it, and a reader of any of them should find it where the
/// operand is interpreted.
///
/// `BuildCache::new` CREATES the directories it is handed, so resolving a root
/// from a typo'd path would manufacture the empty cache it then reports — D5's
/// failure mode with an extra step, and for `clean` it would "clear" a cache it
/// had just made.
pub(crate) fn operand_error(file: Option<&Path>) -> Option<String> {
    match file {
        Some(f) if !f.exists() => Some(format!("no such file: {}", f.display())),
        _ => None,
    }
}

/// The project root for an entry file: its parent directory.
///
/// Mirrors `prepare_project_with_cache`'s `path.parent().unwrap_or(".")`, and
/// the `inspect_cache` refinement that a bare filename's parent is `Some("")`
/// rather than a readable directory — which would otherwise resolve the root to
/// the empty path and inspect nothing.
pub(crate) fn root_from_entry(entry: &Path) -> PathBuf {
    match entry.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Resolve the root a cache command should inspect.
///
/// With an operand, the entry file is canonicalized first so a relative
/// spelling reports the same root the writer used; a path that cannot be
/// canonicalized falls back to its lexical parent rather than failing here —
/// the caller checks existence, and the reported root makes the fallback visible.
pub(crate) fn resolve(entry: Option<&Path>, cwd: &Path) -> CacheRoot {
    match entry {
        Some(e) => {
            let canonical = e.canonicalize().unwrap_or_else(|_| e.to_path_buf());
            CacheRoot {
                path: root_from_entry(&canonical),
                source: RootSource::EntryFile,
            }
        }
        None => CacheRoot {
            path: cwd.to_path_buf(),
            source: RootSource::Cwd,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- operand validation ------------------------------------------------

    #[test]
    fn no_operand_is_not_an_error() {
        assert_eq!(operand_error(None), None);
    }

    #[test]
    fn an_existing_operand_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("main.tg");
        std::fs::write(&f, "fn main() -> Nat { 0 }").unwrap();
        assert_eq!(operand_error(Some(&f)), None);
    }

    #[test]
    fn a_missing_operand_is_refused_by_name() {
        // Refusing beats resolving: `BuildCache::new` would CREATE the directory.
        let msg = operand_error(Some(Path::new("/definitely/not/here/main.tg")))
            .expect("a nonexistent operand must be refused");
        assert_eq!(msg, "no such file: /definitely/not/here/main.tg");
    }

    // --- root resolution ---------------------------------------------------

    #[test]
    fn root_from_entry_is_the_parent_directory() {
        assert_eq!(
            root_from_entry(Path::new("/a/b/main.tg")),
            PathBuf::from("/a/b")
        );
    }

    #[test]
    fn root_from_entry_of_a_bare_filename_is_dot() {
        // `Path::new("main.tg").parent()` is `Some("")`, not `None` — resolving
        // the root to the empty path would inspect nothing.
        assert_eq!(root_from_entry(Path::new("main.tg")), PathBuf::from("."));
    }

    #[test]
    fn root_from_entry_of_a_root_level_file_is_the_root() {
        assert_eq!(root_from_entry(Path::new("/main.tg")), PathBuf::from("/"));
    }

    #[test]
    fn no_operand_resolves_to_the_cwd_and_says_so() {
        let resolved = resolve(None, Path::new("/work/repo"));
        assert_eq!(resolved.path, PathBuf::from("/work/repo"));
        assert_eq!(resolved.source, RootSource::Cwd);
    }

    #[test]
    fn an_operand_resolves_to_the_entry_files_directory_not_the_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("compiler");
        std::fs::create_dir(&sub).unwrap();
        let entry = sub.join("main.tg");
        std::fs::write(&entry, "fn main() -> Nat { 0 }").unwrap();

        let resolved = resolve(Some(&entry), Path::new("/somewhere/else"));

        assert_eq!(resolved.source, RootSource::EntryFile);
        // Canonicalized, so compare against the canonical form of the same dir
        // (macOS resolves /var -> /private/var).
        assert_eq!(resolved.path, sub.canonicalize().unwrap());
        assert_ne!(resolved.path, PathBuf::from("/somewhere/else"));
    }

    #[test]
    fn an_uncanonicalizable_operand_falls_back_to_its_lexical_parent() {
        let resolved = resolve(
            Some(Path::new("/definitely/not/here/main.tg")),
            Path::new("/work/repo"),
        );
        assert_eq!(resolved.path, PathBuf::from("/definitely/not/here"));
        assert_eq!(resolved.source, RootSource::EntryFile);
    }

    #[test]
    fn provenance_names_the_operand_for_an_entry_file_root() {
        let resolved = resolve(Some(Path::new("src/compiler/main.tg")), Path::new("/w"));
        let label = resolved.provenance(Some(Path::new("src/compiler/main.tg")));
        assert_eq!(label, "(from src/compiler/main.tg)");
    }

    #[test]
    fn provenance_for_a_cwd_root_says_how_to_target_a_project() {
        let resolved = resolve(None, Path::new("/w"));
        let label = resolved.provenance(None);
        assert!(label.contains("current directory"), "got {label}");
        assert!(label.contains("pass a file"), "got {label}");
    }
}
