//! `tungsten doctor check module-overlap` — detect `foo.rs` + `foo/mod.rs` coexistence.
//!
//! Walks Rust source directories and reports any module that has both a standalone
//! `.rs` file and a directory module with `mod.rs`. This prevents Rust compiler
//! error E0761 cascades during complexity-driven file splits.
//!
//! See ADR 12.5.26g for design rationale.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A detected `foo.rs` + `foo/mod.rs` overlap.
#[derive(Debug)]
struct Overlap {
    module: String,
    file: PathBuf,
    mod_file: PathBuf,
}

/// Default scan roots when no explicit `--path` is provided.
const DEFAULT_ROOTS: &[&str] = &["bootstrap/src", "tungsten_codegen/src"];

/// Entry point for `tungsten doctor check module-overlap`.
pub fn cmd_check_module_overlap(path: Option<&Path>, json: bool) -> ExitCode {
    let roots: Vec<PathBuf> = if let Some(p) = path {
        if !p.exists() {
            eprintln!("error: path does not exist: {}", p.display());
            return ExitCode::FAILURE;
        }
        vec![p.to_path_buf()]
    } else {
        DEFAULT_ROOTS
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    };

    let mut overlaps = Vec::new();
    for root in &roots {
        find_overlaps_recursive(root, &mut overlaps);
    }
    overlaps.sort_by(|a, b| a.file.cmp(&b.file));

    if json {
        print_json(&overlaps);
    } else if !overlaps.is_empty() {
        print_human(&overlaps);
    }

    if overlaps.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Recursively walk `dir` and collect `foo.rs` + `foo/mod.rs` overlaps.
fn find_overlaps_recursive(dir: &Path, overlaps: &mut Vec<Overlap>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.is_file() {
            files.push(path);
        }
    }

    // Check each .rs file for a sibling directory with mod.rs
    for file in &files {
        if file.extension().map_or(true, |e| e != "rs") {
            continue;
        }
        let stem = match file.file_stem() {
            Some(s) => s,
            None => continue,
        };
        // Skip mod.rs itself
        if stem == "mod" {
            continue;
        }
        let sibling_dir = dir.join(stem);
        let mod_file = sibling_dir.join("mod.rs");
        if mod_file.exists() {
            overlaps.push(Overlap {
                module: stem.to_string_lossy().into_owned(),
                file: file.clone(),
                mod_file,
            });
        }
    }

    // Recurse into subdirectories
    for subdir in &subdirs {
        find_overlaps_recursive(subdir, overlaps);
    }
}

fn print_human(overlaps: &[Overlap]) {
    println!("module-overlap: found Rust E0761 risk\n");
    for o in overlaps {
        println!("module `{}` has both:", o.module);
        println!("  {}", o.file.display());
        println!("  {}", o.mod_file.display());
        println!();
    }
}

fn print_json(overlaps: &[Overlap]) {
    let status = if overlaps.is_empty() { "ok" } else { "failed" };
    let items: Vec<serde_json::Value> = overlaps
        .iter()
        .map(|o| {
            serde_json::json!({
                "module": o.module,
                "file": o.file.display().to_string(),
                "mod_file": o.mod_file.display().to_string(),
            })
        })
        .collect();
    let output = serde_json::json!({
        "check": "module-overlap",
        "status": status,
        "overlaps": items,
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper to create a temp directory with a specific structure.
    fn setup_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn test_clean_directory_no_overlaps() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("foo")).unwrap();
        fs::write(src.join("foo/mod.rs"), "// ok").unwrap();
        fs::write(src.join("bar.rs"), "// ok").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert!(overlaps.is_empty(), "no overlaps expected in clean dir");
    }

    #[test]
    fn test_detects_overlap() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("abi")).unwrap();
        fs::write(src.join("abi.rs"), "// stale").unwrap();
        fs::write(src.join("abi/mod.rs"), "// new").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].module, "abi");
    }

    #[test]
    fn test_detects_nested_overlap() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("codegen/exec")).unwrap();
        fs::write(src.join("codegen/exec.rs"), "// stale").unwrap();
        fs::write(src.join("codegen/exec/mod.rs"), "// new").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].module, "exec");
    }

    #[test]
    fn test_multiple_overlaps() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();
        fs::write(src.join("alpha.rs"), "").unwrap();
        fs::write(src.join("alpha/mod.rs"), "").unwrap();
        fs::write(src.join("beta.rs"), "").unwrap();
        fs::write(src.join("beta/mod.rs"), "").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        overlaps.sort_by(|a, b| a.module.cmp(&b.module));
        assert_eq!(overlaps.len(), 2);
        assert_eq!(overlaps[0].module, "alpha");
        assert_eq!(overlaps[1].module, "beta");
    }

    #[test]
    fn test_mod_rs_itself_not_flagged() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        // A directory named "mod" with mod.rs inside should not trigger
        fs::create_dir_all(src.join("foo")).unwrap();
        fs::write(src.join("foo/mod.rs"), "").unwrap();
        // No foo.rs — clean
        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert!(overlaps.is_empty());
    }

    #[test]
    fn test_missing_explicit_path_exits_failure() {
        let result = cmd_check_module_overlap(
            Some(Path::new("/nonexistent/path/that/does/not/exist")),
            false,
        );
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[test]
    fn test_json_clean_output() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("clean.rs"), "").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert!(overlaps.is_empty());
        // Verify JSON structure by constructing it the same way print_json does
        let status = if overlaps.is_empty() { "ok" } else { "failed" };
        let items: Vec<serde_json::Value> = Vec::new();
        let output = serde_json::json!({
            "check": "module-overlap",
            "status": status,
            "overlaps": items,
        });
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["check"], "module-overlap");
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["overlaps"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_json_dirty_output() {
        let tmp = setup_temp_dir();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("abi")).unwrap();
        fs::write(src.join("abi.rs"), "// stale").unwrap();
        fs::write(src.join("abi/mod.rs"), "// new").unwrap();

        let mut overlaps = Vec::new();
        find_overlaps_recursive(&src, &mut overlaps);
        assert_eq!(overlaps.len(), 1);

        let status = "failed";
        let items: Vec<serde_json::Value> = overlaps
            .iter()
            .map(|o| {
                serde_json::json!({
                    "module": o.module,
                    "file": o.file.display().to_string(),
                    "mod_file": o.mod_file.display().to_string(),
                })
            })
            .collect();
        let output = serde_json::json!({
            "check": "module-overlap",
            "status": status,
            "overlaps": items,
        });
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["check"], "module-overlap");
        assert_eq!(parsed["status"], "failed");
        let arr = parsed["overlaps"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["module"], "abi");
        assert!(arr[0]["file"].as_str().unwrap().ends_with("abi.rs"));
        assert!(arr[0]["mod_file"].as_str().unwrap().ends_with("abi/mod.rs"));
    }
}
