//! Codegen bridge for `audit-recursion` (ADR 1.7.26b §2.2).
//!
//! `audit-recursion` classifies recursion at the source level. On its own it
//! marks every tail-recursive function `✓ musttail eligible` — which is *wrong*
//! for any function the backend silently SKIPs (struct return / non-flattenable
//! struct param). This module carries the codegen verdict that downgrades those
//! over-optimistic ✓s, plus the `analysis_mode` that records whether the
//! backend was actually consulted.

use std::collections::HashMap;

/// The actual codegen musttail verdict for one function.
#[derive(Debug, Clone)]
pub struct CodegenVerdict {
    /// True if musttail was skipped for this function (grows stack).
    pub skipped: bool,
    /// Human reason summary for a skip (empty when not skipped).
    pub reason: String,
}

/// How the musttail verdict column was derived — always reported so a verdict
/// is never ambiguous about whether the backend was consulted.
#[derive(Debug, Clone)]
pub enum AnalysisMode {
    /// Codegen ran; the verdict reflects the actual `check_musttail_abi_safety` gate.
    Codegen,
    /// Built without codegen (`--no-default-features`) or `--source-only`:
    /// source-level estimate only.
    SourceOnly,
    /// Codegen was available but errored (independently of the frontend
    /// classification); source-level fallback carrying the backend error.
    CodegenFailed(String),
}

impl AnalysisMode {
    /// Print the mode banner / caveat (ADR 1.7.26b §2.2).
    pub fn print_banner(&self) {
        match self {
            AnalysisMode::Codegen => {
                println!("analysis_mode: codegen (musttail verdict reflects the actual gate)");
            }
            AnalysisMode::SourceOnly => {
                println!(
                    "analysis_mode: source-only \
                     (codegen not consulted — source-level estimate only)"
                );
            }
            AnalysisMode::CodegenFailed(err) => {
                println!(
                    "analysis_mode: codegen-failed \
                     (codegen failed: {err} — source-level estimate only)"
                );
            }
        }
    }
}

/// Resolve the display symbol + suffix for a tail-recursive function given the
/// codegen verdicts. Returns `None` to keep the default `✓` (musttail applied
/// or no verdict available); `Some(("✗", " SKIP: …  O(N) stack"))` to downgrade.
pub fn tail_override(
    name: &str,
    verdicts: &HashMap<String, CodegenVerdict>,
) -> Option<(&'static str, String)> {
    let v = verdicts.get(name)?;
    if v.skipped {
        Some(("✗", format!("   SKIP: {}  O(N) stack", v.reason)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_verdict_downgrades_to_cross() {
        let mut v = HashMap::new();
        v.insert(
            "collect_type_names".to_string(),
            CodegenVerdict {
                skipped: true,
                reason: "struct param+ret".into(),
            },
        );
        let (sym, suffix) = tail_override("collect_type_names", &v).unwrap();
        assert_eq!(sym, "✗");
        assert!(suffix.contains("SKIP: struct param+ret"));
        assert!(suffix.contains("O(N) stack"));
    }

    #[test]
    fn emit_verdict_keeps_default_check() {
        let mut v = HashMap::new();
        v.insert(
            "loop".to_string(),
            CodegenVerdict {
                skipped: false,
                reason: String::new(),
            },
        );
        assert!(tail_override("loop", &v).is_none());
    }

    #[test]
    fn absent_verdict_keeps_default_check() {
        let v = HashMap::new();
        assert!(tail_override("anything", &v).is_none());
    }
}
