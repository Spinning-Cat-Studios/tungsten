//! Output comparison + normalization for the golden runner. Split out of
//! `main.rs` (ADR 16.7.26b file-size paydown). Strips ANSI colour codes and
//! rewrites absolute paths to relative before comparing actual vs `.expected`.

use std::fs;
use std::path::Path;

pub(crate) enum TestResult {
    Pass,
    Fail,
    /// The `.tg` exists but its `.expected` snapshot does not.
    ///
    /// Counted apart from `Fail` because the remedy differs — `--update`
    /// generates the snapshot, whereas a `Fail` needs the diff read — but it
    /// is **not** a pass. It used to be: a fixture added without running
    /// `--update` printed `? (no expected file)`, was counted in "N passed"
    /// and the runner exited 0, so the fixture was invisible to CI while
    /// looking green locally. Found by ADR 7.8.26e's non-vacuity work, where
    /// seven brand-new `error/` fixtures reported `39 passed, 0 failed`
    /// before any snapshot existed.
    MissingSnapshot,
}

pub(crate) fn compare_output(tg: &Path, expected_path: &Path, actual: &str) -> TestResult {
    if !expected_path.exists() {
        println!(
            "\x1b[31m?\x1b[0m {} (no expected file — run with --update)",
            tg.display()
        );
        return TestResult::MissingSnapshot;
    }

    let expected = fs::read_to_string(expected_path).unwrap_or_default();
    let expected = expected.trim_end();
    let actual = actual.trim_end();

    if expected == actual {
        println!("\x1b[32m✓\x1b[0m {}", tg.display());
        TestResult::Pass
    } else {
        println!("\x1b[31m✗\x1b[0m {}", tg.display());
        print_diff(expected, actual);
        TestResult::Fail
    }
}

fn print_diff(expected: &str, actual: &str) {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();

    let max = exp_lines.len().max(act_lines.len()).min(10);
    let mut shown_diff = false;

    for i in 0..max {
        let e = exp_lines.get(i).copied().unwrap_or("");
        let a = act_lines.get(i).copied().unwrap_or("");
        if e != a && !shown_diff {
            println!("  First diff at line {}:", i + 1);
            println!("    expected: {}", truncate(e, 120));
            println!("    actual:   {}", truncate(a, 120));
            shown_diff = true;
        }
    }

    if !shown_diff {
        // Trailing whitespace / newline difference
        println!("  (trailing whitespace/newline difference)");
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

/// Strip ANSI escape sequences from output.
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC [ ... m sequences
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc == 'm' {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Normalize absolute paths to relative.
pub(crate) fn normalize_paths(s: &str) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        let prefix = format!("{}/", cwd.display());
        s.replace(&prefix, "")
    } else {
        s.to_string()
    }
}
