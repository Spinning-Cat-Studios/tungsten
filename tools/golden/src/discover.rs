//! Golden test discovery. Split out of `main.rs` (ADR 16.7.26b file-size
//! paydown). Finds both single-file tests (`*.tg` in the category dir) and
//! multi-file tests (subdirectories holding a `main.tg`).

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Discover test files in a category directory.
/// Returns (tg_file, expected_file) pairs.
pub(crate) fn discover_tests(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut tests = Vec::new();

    if !dir.exists() {
        return tests;
    }

    // Single-file tests: *.tg in the directory
    if let Ok(entries) = fs::read_dir(dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("tg")))
            .map(|e| e.path())
            .collect();
        files.sort();
        for tg in files {
            let expected = tg.with_extension("expected");
            tests.push((tg, expected));
        }
    }

    // Multi-file tests: subdirectories with main.tg
    if let Ok(entries) = fs::read_dir(dir) {
        let mut subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| !e.file_name().to_str().map_or(true, |n| n.starts_with('.')))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        for subdir in subdirs {
            let main_tg = subdir.join("main.tg");
            let expected = subdir.join("main.expected");
            if main_tg.exists() {
                tests.push((main_tg, expected));
            }
        }
    }

    tests
}
