//! Tier 2: `llc` integration for register assignment analysis.

use super::LlcRegisterInfo;
use std::path::Path;

/// Run llc to get register assignment information.
pub(super) fn run_llc_analysis(file: &Path) -> Option<LlcRegisterInfo> {
    // Try llc-18 first, then llc
    let llc_binary = find_llc_binary()?;

    let out = std::process::Command::new(&llc_binary)
        .args([
            "-mtriple=aarch64-linux-gnu",
            "--debug-only=aarch64-call-lowering",
        ])
        .arg(file)
        .arg("-o")
        .arg("/dev/null")
        .output()
        .map_err(|e| {
            eprintln!("warning: could not invoke llc: {e}");
            eprintln!("note: --deep requires llc in PATH");
        })
        .ok()?;

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stderr.trim().is_empty() {
        return Some(LlcRegisterInfo { raw_output: stderr });
    }

    // Fallback: try irtranslator
    let out2 = std::process::Command::new(&llc_binary)
        .args(["-mtriple=aarch64-linux-gnu", "--debug-only=irtranslator"])
        .arg(file)
        .arg("-o")
        .arg("/dev/null")
        .output()
        .map_err(|e| {
            eprintln!("warning: llc invocation failed: {e}");
        })
        .ok()?;

    let stderr2 = String::from_utf8_lossy(&out2.stderr).to_string();
    if stderr2.trim().is_empty() {
        eprintln!("note: llc produced no debug output for call lowering");
        None
    } else {
        Some(LlcRegisterInfo {
            raw_output: stderr2,
        })
    }
}

/// Locate the llc binary (llc-18 preferred, fallback to llc).
fn find_llc_binary() -> Option<String> {
    // Check well-known paths first (devcontainer)
    let well_known = "/usr/lib/llvm-18/bin/llc";
    if std::path::Path::new(well_known).exists() {
        return Some(well_known.to_string());
    }

    // Try llc-18 in PATH
    if std::process::Command::new("llc-18")
        .arg("--version")
        .output()
        .is_ok()
    {
        return Some("llc-18".to_string());
    }

    // Try llc in PATH
    if std::process::Command::new("llc")
        .arg("--version")
        .output()
        .is_ok()
    {
        return Some("llc".to_string());
    }

    eprintln!("warning: llc not found (tried /usr/lib/llvm-18/bin/llc, llc-18, llc)");
    eprintln!("note: --deep requires llc in PATH");
    None
}
