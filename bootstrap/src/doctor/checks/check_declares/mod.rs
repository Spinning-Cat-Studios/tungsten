//! Declaration hygiene check for per-unit LLVM IR (ADR 10.5.26b §2.2).
//!
//! Scans `.ll` files and validates that every direct `call @symbol` target
//! has a matching `declare` or `define` in the same file.

mod scanner;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::ExitCode;

/// Run the check-declares scan on all `.ll` files in a directory.
pub fn cmd_check_declares(dir: &Path) -> ExitCode {
    let ll_files = match collect_ll_files(dir) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("error: cannot read directory {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };

    if ll_files.is_empty() {
        eprintln!("error: no .ll files found in {}", dir.display());
        return ExitCode::FAILURE;
    }

    println!(
        "Checking {} .ll file(s) for declaration hygiene...",
        ll_files.len()
    );

    let mut total_missing = 0;
    let mut files_with_errors = 0;

    for file in &ll_files {
        let ir = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(e) => {
                eprintln!("  ✗ {}: read error: {e}", file.display());
                files_with_errors += 1;
                continue;
            }
        };

        let missing = scanner::find_missing_declarations(&ir);
        if !missing.is_empty() {
            files_with_errors += 1;
            for m in &missing {
                let rel = file.strip_prefix(dir).unwrap_or(file);
                eprintln!(
                    "  ✗ {}: call @{} has no declare or define (line {})",
                    rel.display(),
                    m.symbol,
                    m.line_number,
                );
            }
            total_missing += missing.len();
        }
    }

    if total_missing == 0 {
        println!("✓ All call targets have matching declare/define statements.");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!(
            "{} missing declaration(s) in {} file(s).",
            total_missing, files_with_errors,
        );
        ExitCode::FAILURE
    }
}

/// Recursively collect all `.ll` files in a directory.
fn collect_ll_files(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_ll_files_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_ll_files_recursive(
    dir: &Path,
    files: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_ll_files_recursive(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "ll") {
            files.push(path);
        }
    }
    Ok(())
}
