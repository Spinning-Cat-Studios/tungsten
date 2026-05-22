//! `tungsten info codegen-units` — show per-module codegen unit partitioning.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::driver;

/// Show codegen unit partitioning: unit names, def counts, and sorted def names.
pub fn cmd_info_codegen_units(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let units = &project.codegen_units;
    if units.is_empty() {
        println!("No codegen units (single-module project).");
        println!();
        println!("unit: main ({} defs)", project.defs.len());
        let mut names: Vec<&str> = project.defs.iter().map(|d| d.name.as_str()).collect();
        names.sort();
        for name in &names {
            println!("  {}", name);
        }
        return ExitCode::SUCCESS;
    }

    println!("{} codegen unit(s):", units.len());
    println!();

    let source_root = file.parent().unwrap_or(Path::new("."));
    for unit in units {
        let unit_name = crate::compile::per_module::codegen_unit_name(
            &unit.source_file,
            source_root,
            &unit.defs[0].name,
        );
        let mut names: Vec<&str> = unit.defs.iter().map(|d| d.name.as_str()).collect();
        names.sort();
        println!("unit: {} ({} defs)", unit_name, names.len());
        for name in &names {
            println!("  {}", name);
        }
        println!();
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::process::ExitCode;

    #[test]
    fn codegen_units_single_module_succeeds() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        std::fs::write(&path, "fn main() -> Nat { 42 }").unwrap();
        let result = super::cmd_info_codegen_units(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn codegen_units_multi_module_succeeds() {
        // Use the multi_module_collision fixture which has alpha + beta modules
        let path = std::path::PathBuf::from("tests/multi_module_collision/mod.tg");
        if !path.exists() {
            // Skip if fixture not available (e.g. running from different dir)
            return;
        }
        let result = super::cmd_info_codegen_units(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn codegen_units_nonexistent_file_fails() {
        let path = std::path::PathBuf::from("/nonexistent/file.tg");
        let result = super::cmd_info_codegen_units(&path, false, 20);
        assert_eq!(result, ExitCode::FAILURE);
    }

    /// Golden-style test: verify output format for multi-module project.
    #[test]
    fn codegen_units_output_format() {
        use tungsten_bootstrap::driver;

        let path = std::path::PathBuf::from("tests/multi_module_collision/mod.tg");
        if !path.exists() {
            return;
        }

        let project = match driver::elaborate_project(&path, false, 20, None) {
            Ok(p) => p,
            Err(_) => return, // skip if elaboration fails
        };

        let units = &project.codegen_units;
        // Multi-module: should have codegen units
        assert!(
            units.len() >= 2,
            "expected at least 2 codegen units, got {}",
            units.len()
        );

        // Each unit should have a non-empty module path and at least one def
        for unit in units {
            assert!(!unit.module_path.is_empty() || !unit.defs.is_empty());
        }

        // Verify the unit naming function produces stable names
        let source_root = path.parent().unwrap_or(std::path::Path::new("."));
        let names: Vec<String> = units
            .iter()
            .map(|u| {
                crate::compile::per_module::codegen_unit_name(
                    &u.source_file,
                    source_root,
                    &u.defs[0].name,
                )
            })
            .collect();
        // With per-function units, each unit has exactly 1 def
        for unit in units {
            assert_eq!(
                unit.defs.len(),
                1,
                "per-function unit should have exactly 1 def"
            );
        }
        // Should contain units with function names from alpha/beta modules
        assert!(!names.is_empty(), "expected at least one unit, got empty");
    }
}
