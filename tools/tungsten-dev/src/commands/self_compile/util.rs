//! File discovery + parallelism resolution helpers for self-compile.

use std::io;
use std::path::Path;

pub fn find_files(dir: &Path, extension: &str) -> Result<Vec<String>, io::Error> {
    let mut files = Vec::new();
    collect_files(dir, extension, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(dir: &Path, extension: &str, out: &mut Vec<String>) -> Result<(), io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, out)?;
        } else if path.extension().is_some_and(|ext| ext == extension) {
            out.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

/// Resolve parallelism: --parallelism > TUNGSTEN_CODEGEN_JOBS env > nproc/2.
/// Returns at least 1.
pub fn resolve_parallelism(explicit: Option<usize>) -> usize {
    let env_val = std::env::var("TUNGSTEN_CODEGEN_JOBS").ok();
    resolve_parallelism_with_env(explicit, env_val.as_deref())
}

/// Inner implementation that takes the env value as a parameter.
/// Avoids `std::env::set_var` / `remove_var` unsoundness in tests.
fn resolve_parallelism_with_env(explicit: Option<usize>, env_val: Option<&str>) -> usize {
    if let Some(n) = explicit {
        return n.max(1);
    }
    if let Some(val) = env_val {
        if let Ok(n) = val.parse::<usize>() {
            return n.max(1);
        }
    }
    default_parallelism()
}

/// OOM-safe default: max(1, nproc / 2).
fn default_parallelism() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (cpus / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_files_discovers_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        fs::write(base.join("a.ll"), "").unwrap();
        fs::write(base.join("b.ll"), "").unwrap();
        fs::write(base.join("c.o"), "").unwrap();
        fs::create_dir(base.join("sub")).unwrap();
        fs::write(base.join("sub/d.ll"), "").unwrap();

        let ll_files = find_files(base, "ll").unwrap();
        assert_eq!(ll_files.len(), 3);
        assert!(ll_files.iter().all(|f| f.ends_with(".ll")));

        let o_files = find_files(base, "o").unwrap();
        assert_eq!(o_files.len(), 1);
    }

    #[test]
    fn find_files_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_files(dir.path(), "ll").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn find_files_nonexistent_dir_errors() {
        let result = find_files(Path::new("/nonexistent/path/xyz"), "ll");
        assert!(result.is_err());
    }

    #[test]
    fn default_parallelism_is_half_cpus() {
        let p = default_parallelism();
        assert!(p >= 1, "parallelism must be at least 1");
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        assert_eq!(p, (cpus / 2).max(1));
    }

    #[test]
    fn resolve_parallelism_explicit_wins() {
        let p = resolve_parallelism_with_env(Some(4), Some("99"));
        assert_eq!(p, 4);
    }

    #[test]
    fn resolve_parallelism_env_overrides_default() {
        let p = resolve_parallelism_with_env(None, Some("7"));
        assert_eq!(p, 7);
    }

    #[test]
    fn resolve_parallelism_falls_back_to_default() {
        let p = resolve_parallelism_with_env(None, None);
        assert_eq!(p, default_parallelism());
    }

    #[test]
    fn resolve_parallelism_clamps_to_one() {
        assert_eq!(resolve_parallelism_with_env(Some(0), None), 1);
        assert_eq!(resolve_parallelism_with_env(None, Some("0")), 1);
    }

    #[test]
    fn resolve_parallelism_ignores_invalid_env() {
        let p = resolve_parallelism_with_env(None, Some("banana"));
        assert_eq!(p, default_parallelism());
    }
}
