//! Post-link smoke gate (ADR 2.7.26b T4a).
//!
//! A self-compile is not "built" until its product has been RUN: the 1.7.26e
//! §6.6 miscompile linked cleanly and crashed in 19 ms on first use, yet
//! shipped behind a "✓ Built" line. Every self-compile mode therefore ends
//! with `<tungsten1> check examples/hello.tg`; a non-zero exit (or crash)
//! fails the self-compile and surfaces the exact command + exit status.

use std::path::Path;
use std::process::Command;

/// Run `<binary> check <input>`; error on launch failure or exit ≠ 0.
pub fn smoke_check(binary: &Path, input: &Path) -> Result<(), String> {
    let cmd_line = format!("{} check {}", binary.display(), input.display());
    eprintln!("\n=== Smoke check: {cmd_line} ===");

    let output = Command::new(binary)
        .arg("check")
        .arg(input)
        .output()
        .map_err(|e| format!("smoke check could not launch `{cmd_line}`: {e}"))?;

    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        return Err(format!(
            "smoke check FAILED: `{cmd_line}` exited with {code} — the freshly \
             built compiler does not run; do not trust this build. stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    eprintln!("✓ Smoke check passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn stub_compiler(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// T4a: a stub compiler that EXISTS and is EXECUTABLE but exits non-zero
    /// during the smoke check must fail the self-compile, surfacing the smoke
    /// command and the exit status. (The §6.6 regression was a binary that ran
    /// and crashed — not a missing path.)
    #[test]
    fn crashing_binary_fails_with_command_and_status() {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub_compiler(dir.path(), "tungsten1", "echo boom >&2; exit 3");
        let input = dir.path().join("hello.tg");
        fs::write(&input, "").unwrap();

        let err = smoke_check(&bin, &input).unwrap_err();
        assert!(err.contains("exited with 3"), "exit status surfaced: {err}");
        assert!(err.contains("tungsten1"), "smoke command surfaced: {err}");
        assert!(err.contains("check"), "smoke command surfaced: {err}");
        assert!(err.contains("boom"), "stderr surfaced: {err}");
    }

    #[test]
    fn healthy_binary_passes() {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub_compiler(dir.path(), "tungsten1", "exit 0");
        let input = dir.path().join("hello.tg");
        fs::write(&input, "").unwrap();

        assert!(smoke_check(&bin, &input).is_ok());
    }

    #[test]
    fn missing_binary_is_launch_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = smoke_check(&dir.path().join("nope"), &dir.path().join("hello.tg")).unwrap_err();
        assert!(
            err.contains("could not launch"),
            "launch failure surfaced: {err}"
        );
    }
}
