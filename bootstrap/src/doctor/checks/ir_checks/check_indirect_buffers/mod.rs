//! `tungsten doctor check ir indirect-buffers` — audit Class-P indirect-param
//! buffer discipline in emitted LLVM IR (ADR 1.7.26e §2.5/§2.6, R4/R10; ADR
//! 17.7.26e §2.2).
//!
//! Since ADR 17.7.26e this is the standing **`noalias` oracle**: the
//! indirect-param slots carry `noalias`, whose soundness rests on the buffer
//! bytes being *single-routed* (invariants I1–I5). The audit re-checks that on
//! every emitted `.ll` corpus, in two arms.
//!
//! **Callee arm** — for each `$direct_mt` function with a self-`musttail call`:
//!   * **R10 / forwarding** — every `ptr` operand of the self-`musttail` is a
//!     callee *parameter* (the forwarded sret / buffer / env pointers) or `null`,
//!     never a fresh `alloca` result (a per-iteration buffer forwarded across the
//!     musttail edge is the unbounded-growth bug ADR 1.7.26e removes).
//!   * **I5 / tail-edge distinctness** — the pointers forwarded into the sret +
//!     indirect-param slots are pairwise distinct, so the next activation
//!     re-enters with two `noalias` params that still cannot alias.
//!   * **Non-escape (derived-address audit, R4)** — the audit tracks the buffer
//!     pointer's *transitive address-users*: addresses derived via
//!     `getelementptr`/`bitcast`/`addrspacecast` are tracked too. **Permitted**
//!     uses: `load`/`store` *through* a tracked address, address derivation,
//!     `llvm.memcpy`/`llvm.memmove`, `llvm.lifetime.*`, debug intrinsics, and
//!     the forwarding self-`musttail` argument. **Escapes**: a tracked pointer
//!     (or derived address) stored as a *value* (`store ptr %p, …`), returned
//!     (`ret ptr %p`), or passed to any *non-forwarding* call.
//!
//! **Shim arm (I4)** — for each `$direct` shim that calls a `$direct_mt` entry
//! with buffer slots: its `sret_buf` / `indirect_buf.*` allocas may be used only
//! for {the defining alloca, address derivation, `llvm.lifetime.*`/debug/memory
//! intrinsics, the fill store *through* the buffer, the `$direct_mt` call
//! argument, and the sret read-back load}. Anything else is a
//! [`FindingKind::ShimBufferEscape`].
//!
//! Text-based (like `check ir declares`/`null-calls`): run `tungsten compile
//! --emit-llvm` first, then point this at the output directory.

use std::path::Path;
use std::process::ExitCode;

use super::corpus::{reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts};

mod callee_arm;
mod parse;
mod shim_arm;
mod track;
use parse::split_functions;

// Tests: tests/callee_arm_tests.rs, tests/shim_tests.rs, tests/summary_tests.rs
//
// Three files (this audit has three surfaces to pin and one file would outgrow
// the size gate), grouped into `tests/` so this directory stays under the
// file-count gate — ADR 28.7.26e's close-out hit that cap and had to append to
// the wrong file. `#[path]` keeps them DIRECT children of this module, so
// `use super::*` still reaches the private items they assert on.
#[cfg(test)]
#[path = "tests/callee_arm_tests.rs"]
mod tests;
/// Tests for the ADR 17.7.26e arms (shim I4 + tail-edge I5).
#[cfg(test)]
#[path = "tests/shim_tests.rs"]
mod tests_shim;
/// Tests for the summary surface: per-arm counters, their aggregation, and
/// vacuous-pass detection.
#[cfg(test)]
#[path = "tests/summary_tests.rs"]
mod tests_summary;

/// A single buffer-discipline violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferFinding {
    pub function: String,
    pub kind: FindingKind,
    /// The offending IR line (trimmed).
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// A `musttail` argument is a fresh `alloca` result, not a forwarded param.
    ForwardedAlloca,
    /// A buffer/sret pointer (or an address derived from it) escapes: stored as
    /// a value, returned, or passed to a non-forwarding call.
    PointerEscape,
    /// A `$direct` shim uses a buffer address outside the I4 allowlist.
    ShimBufferEscape,
    /// A self-`musttail` edge forwards the same pointer into two buffer slots,
    /// aliasing two `noalias` params on the next activation (I5).
    TailEdgeAliasedForward,
}

impl FindingKind {
    fn human(self) -> &'static str {
        match self {
            FindingKind::ForwardedAlloca => "musttail forwards an alloca (R10: per-iteration buffer)",
            FindingKind::PointerEscape => {
                "buffer pointer (or derived address) escapes (stored-as-value, returned, or passed to a non-forwarding call — R4)"
            }
            FindingKind::ShimBufferEscape => {
                "shim buffer address used outside the allowlist (alloca / derivation / lifetime / fill store / $direct_mt arg / sret read-back — I4)"
            }
            FindingKind::TailEdgeAliasedForward => {
                "musttail edge forwards one pointer into two buffer slots — aliased noalias params on the next activation (I5)"
            }
        }
    }
}

/// A finding together with the `.ll` file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedFinding {
    pub file: String,
    pub finding: BufferFinding,
}

/// Aggregate result of auditing one IR module (ADR 2.7.26b T5a): the findings
/// plus the per-arm counts needed to detect a vacuous pass.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AuditSummary {
    pub findings: Vec<BufferFinding>,
    /// The `$direct_mt` callee arm. A candidate is a definition whose signature
    /// carries an indirect aggregate / sret buffer parameter; a scalar-only
    /// `$direct_mt` legitimately forwards zero buffers and is not one. Vacuity
    /// here is the 1.7.26e §6.5 failure mode.
    pub callee: ReachCounts,
    /// The `$direct` shim arm. A candidate is recognized by its attributed call
    /// into a `$direct_mt` entry — *without* consulting the buffer alloca names,
    /// so a `shim.rs` rename shows up as vacuity (ADR 17.7.26e §2.2).
    pub shim: ReachCounts,
}

impl AuditSummary {
    /// True when EITHER arm found functions it was expected to track buffers in,
    /// yet tracked none — a pass that proves nothing (ADR 2.7.26b T5a).
    pub fn is_vacuous(&self) -> bool {
        self.callee.is_vacuous() || self.shim.is_vacuous()
    }

    /// Add one module's per-arm counts into this running total.
    fn add_counts(&mut self, module: &AuditSummary) {
        self.callee.add(module.callee);
        self.shim.add(module.shim);
    }
}

/// Tag a module's findings with the `.ll` file they were found in.
fn locate_findings(findings: Vec<BufferFinding>, file: &Path) -> Vec<LocatedFinding> {
    let file = file.display().to_string();
    findings
        .into_iter()
        .map(|finding| LocatedFinding {
            file: file.clone(),
            finding,
        })
        .collect()
}

/// Scan a directory of `.ll` files for indirect-buffer violations.
///
/// `strict` (used by CI) additionally fails on a vacuous pass: candidate
/// functions exist in an arm but zero buffers were tracked (ADR 2.7.26b T5a).
pub fn cmd_check_indirect_buffers(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_indirect_buffers`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }
    let mut findings: Vec<LocatedFinding> = Vec::new();
    let mut totals = AuditSummary::default();
    let scan = scan_ll_corpus(dir, |path, text| {
        let module = audit_ir_summary(text);
        totals.add_counts(&module);
        findings.extend(locate_findings(module.findings, path));
    });
    // An unreadable `.ll` is charged to the callee arm, so a corpus the audit
    // cannot open reads as drifted rather than clean (ADR 28.7.26e D4).
    scan.charge_unreadable(&mut totals.callee);

    // Machine-readable summary line (per-arm counts, ADR 2.7.26b T5a). The
    // per-arm key names are grandfathered — 17.7.26e's tooling reads them.
    println!(
        "summary: files={} direct_mt_candidates={} tracked_buffers={} shim_candidates={} shim_buffers={} violations={}",
        scan.files,
        totals.callee.candidates,
        totals.callee.tracked,
        totals.shim.candidates,
        totals.shim.tracked,
        findings.len()
    );

    let verdict = AuditVerdict::classify(&scan, totals.is_vacuous(), findings.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => {
            println!("⚠ {n} indirect-buffer violation(s):\n");
            for located in &findings {
                let f = &located.finding;
                println!(
                    "  {}: {} — {}\n    {}",
                    located.file,
                    f.function,
                    f.kind.human(),
                    f.line
                );
            }
            println!("\nSee ADR 1.7.26e §2.5/§2.6 (R4/R10) and ADR 17.7.26e §2.2 (I4/I5).");
        }
        AuditVerdict::Clean => println!(
            "✓ indirect-param buffer discipline OK in {} (no forwarded allocas, no escapes, no aliased tail-edge slots)",
            dir.display()
        ),
        _ => {}
    }
    verdict.report_vacuity(
        totals.is_vacuous(),
        "an audit arm tracked no buffers across its candidate function(s)",
    );
    verdict
}

/// Audit one LLVM IR module (text) for indirect-buffer violations. Pure +
/// unit-testable: no filesystem.
pub fn audit_ir(text: &str) -> Vec<BufferFinding> {
    audit_ir_summary(text).findings
}

/// Audit one LLVM IR module, also reporting per-arm candidate/tracked counts for
/// vacuous-pass detection (ADR 2.7.26b T5a). Pure + unit-testable.
pub fn audit_ir_summary(text: &str) -> AuditSummary {
    let mut summary = AuditSummary::default();
    for func in split_functions(text) {
        callee_arm::audit_callee(&func, &mut summary);
        shim_arm::audit_shim(&func, &mut summary);
    }
    summary
}
