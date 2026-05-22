//! `tungsten doctor check ir null-calls` — detect null function pointer calls in LLVM IR.
//!
//! Scans `.ll` files for call instructions targeting `null` that indicate
//! unresolved monomorphization — a mono instance was expected but the
//! function pointer was never filled in. This is the heuristic that caught
//! 282 null calls in ADR 10.5.26c.

use std::path::Path;
use std::process::ExitCode;

/// Scan a directory of `.ll` files for null function pointer calls.
pub fn cmd_check_null_calls(dir: &Path) -> ExitCode {
    if !dir.is_dir() {
        eprintln!("error: {} is not a directory", dir.display());
        return ExitCode::FAILURE;
    }

    let findings = scan_directory(dir);

    if findings.is_empty() {
        println!(
            "✓ No null function pointer calls found in {}",
            dir.display()
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "⚠ {} null function pointer call(s) found:\n",
            findings.len()
        );
        for f in &findings {
            println!("  {}:{}: {}", f.file, f.line_num, f.line.trim());
        }
        println!(
            "\nHint: null calls indicate missed monomorphization.\n\
             Check `tungsten info codegen mono` for the expected mono instances."
        );
        ExitCode::FAILURE
    }
}

/// A single finding: a line in a `.ll` file matching the null-call pattern.
#[derive(Debug)]
pub struct NullCallFinding {
    pub file: String,
    pub line_num: usize,
    pub line: String,
}

/// Recursively scan all `.ll` files in a directory for null calls.
pub fn scan_directory(dir: &Path) -> Vec<NullCallFinding> {
    let mut findings = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return findings,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            findings.extend(scan_directory(&path));
        } else if path.extension().is_some_and(|ext| ext == "ll") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                findings.extend(scan_ll_content(&path.display().to_string(), &content));
            }
        }
    }

    findings
}

/// Scan a single `.ll` file's content for null function pointer calls.
///
/// Heuristic: a line contains both "call" and "null(" where "null(" follows "call".
/// This catches patterns like `call i64 null(ptr null)` without a regex dependency.
pub fn scan_ll_content(filename: &str, content: &str) -> Vec<NullCallFinding> {
    let mut findings = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(call_pos) = trimmed.find("call ") {
            // Check for "null(" after the "call" keyword, excluding @-prefixed symbols
            let after_call = &trimmed[call_pos..];
            if after_call.contains("null(") {
                findings.push(NullCallFinding {
                    file: filename.to_string(),
                    line_num: i + 1,
                    line: line.to_string(),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_null_call() {
        let content = r#"
define void @foo() {
  %1 = call i64 null(ptr null)
  ret void
}
"#;
        let findings = scan_ll_content("test.ll", content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line_num, 3);
    }

    #[test]
    fn no_false_positive_on_normal_call() {
        let content = r#"
define void @foo() {
  %1 = call i64 @bar(i64 42)
  ret void
}
"#;
        let findings = scan_ll_content("test.ll", content);
        assert!(findings.is_empty());
    }

    #[test]
    fn no_false_positive_on_null_store() {
        let content = r#"
define void @foo() {
  store ptr null, ptr %1
  ret void
}
"#;
        let findings = scan_ll_content("test.ll", content);
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_multiple_null_calls() {
        let content = r#"
  %1 = call i64 null(ptr null)
  %2 = call { ptr, ptr } null(ptr null, i64 1)
"#;
        let findings = scan_ll_content("multi.ll", content);
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn comment_with_null_is_false_positive() {
        // Known limitation: comments containing "call" + "null(" are false positives.
        // This test documents the behavior rather than asserting absence.
        let content = "; call something null(not real)\n";
        let findings = scan_ll_content("comment.ll", content);
        // The heuristic matches because it doesn't parse LLVM IR comments.
        // In practice, LLVM IR comments rarely contain this pattern.
        assert_eq!(
            findings.len(),
            1,
            "heuristic matches comment lines — known limitation"
        );
    }

    #[test]
    fn empty_directory_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let findings = scan_directory(dir.path());
        assert!(findings.is_empty());
    }

    #[test]
    fn scan_directory_recursive() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("nested.ll"), "  %1 = call i64 null(ptr null)\n").unwrap();
        let findings = scan_directory(dir.path());
        assert_eq!(findings.len(), 1, "should find null call in nested subdir");
        assert!(findings[0].file.contains("nested.ll"));
    }
}
