//! `tungsten doctor check ir sret-stores` — canonical-shape lint for sret
//! returns in emitted LLVM IR (ADR 3.7.26d).
//!
//! **Scope: a shape lint over the compiler's canonical sret lowering, not a
//! general sret ABI verifier.** The codegen emits exactly two legitimate
//! `ret void` shapes in sret functions:
//!
//! 1. epilogue `store <val>, ptr <sret-param>` immediately followed by
//!    `ret void` (`emit_sret_return`);
//! 2. musttail self-tail `musttail call … <sret-param> …; ret void`
//!    (the epilogue forwards the out-pointer).
//!
//! A `ret void` whose immediately preceding instruction is neither — the
//! exact ADR 3.7.26a defect-2 shape (`call …; ret void` discarding the
//! result) — is reported as a *suspicious bare-return shape*, not a proven
//! ABI violation. IR that stores in a predecessor block and branches to a
//! shared return block would be flagged **by design**: a new legitimate
//! lowering shape should update this lint deliberately, not pass silently.
//!
//! **Supported grammar subset** (text-level scan; a header that mentions
//! `sret(` but falls outside this subset fails closed as an
//! `unparseable sret function header` finding, never a silent skip):
//! - quoted (`@"walk$direct_mt"`) and unquoted (`@main`) function names;
//! - named (`%out`) and unnamed (`%0`) parameters — the parameter name is
//!   the last whitespace-separated `%`-token of its comma-separated entry;
//! - `sret(%T)` and `sret({ … })` type spellings, with the attribute in any
//!   position within the parameter entry;
//! - `store` instructions with trailing `, align N` / `, !metadata` suffixes;
//! - `ret void` with optional trailing `, !metadata`;
//! - full-line `;` comments, blank lines, and simple `label:` block headers
//!   (optionally followed by a `; preds = …` comment).
//!
//! Text-based (like `check ir indirect-buffers`): run
//! `tungsten compile --emit-llvm` first, then point this at the output dir.

use std::path::Path;
use std::process::ExitCode;

use super::corpus::{reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts};
use super::textparse::{split_ir_functions, split_top_level_commas, IrFunc};

#[cfg(test)]
mod tests;

/// A single sret-shape finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SretFinding {
    pub function: String,
    pub kind: SretFindingKind,
    /// The offending IR line (trimmed): the bare `ret void` or the header.
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SretFindingKind {
    /// A `ret void` in an sret function whose immediately preceding
    /// instruction is neither a store through the sret parameter nor a
    /// `musttail call` forwarding it.
    BareReturn,
    /// The header mentions `sret(` but (name, sret-param) could not be
    /// bound — fail closed (ADR 3.7.26d parsing contract).
    UnparseableHeader,
}

impl SretFindingKind {
    fn human(self) -> &'static str {
        match self {
            SretFindingKind::BareReturn => {
                "suspicious bare ret void in sret function — result may be discarded \
                 (canonical-shape lint; see ADR 3.7.26a defect 2)"
            }
            SretFindingKind::UnparseableHeader => {
                "unparseable sret function header (fail-closed; ADR 3.7.26d parsing contract)"
            }
        }
    }
}

/// Aggregate result of auditing one IR module: findings plus the parser's reach.
///
/// `candidates` counts functions whose header carries an `sret(…)` parameter
/// and `tracked` the `ret void` sites classified inside them, so a header or
/// return format the parser stops recognizing shows up as vacuity rather than
/// as a clean bill of health (ADR 28.7.26e D4).
#[derive(Default)]
pub struct SretAuditSummary {
    pub findings: Vec<SretFinding>,
    pub returns: ReachCounts,
}

impl SretAuditSummary {
    /// Functions whose header carries an `sret(…)` parameter attribute.
    pub fn sret_functions(&self) -> usize {
        self.returns.candidates
    }
}

/// Scan a directory of `.ll` files for suspicious sret-return shapes.
pub fn cmd_check_sret_stores(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_sret_stores`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }
    let mut findings: Vec<(String, SretFinding)> = Vec::new();
    let mut returns = ReachCounts::default();
    let scan = scan_ll_corpus(dir, |path, text| {
        let module = audit_ir(text);
        returns.add(module.returns);
        for f in module.findings {
            findings.push((path.display().to_string(), f));
        }
    });
    scan.charge_unreadable(&mut returns);

    println!(
        "summary: files={} sret_functions={} returns_classified={} findings={}",
        scan.files,
        returns.candidates,
        returns.tracked,
        findings.len()
    );

    let verdict = AuditVerdict::classify(&scan, returns.is_vacuous(), findings.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => {
            println!("⚠ {n} sret-shape finding(s):\n");
            for (file, f) in &findings {
                println!(
                    "  {file}: {} — {}\n    {}",
                    f.function,
                    f.kind.human(),
                    f.line
                );
            }
            println!("\nSee ADR 3.7.26d (canonical sret lowering shapes).");
        }
        AuditVerdict::Clean => println!(
            "✓ sret-return shapes OK in {} (every ret void stores through or forwards its sret param)",
            dir.display()
        ),
        _ => {}
    }
    verdict.report_vacuity(
        returns.is_vacuous(),
        "sret functions were found but no `ret void` site was classified",
    );
    verdict
}

/// Audit one LLVM IR module (text). Pure + unit-testable: no filesystem.
pub fn audit_ir(text: &str) -> SretAuditSummary {
    let mut summary = SretAuditSummary::default();
    for func in split_ir_functions(text) {
        audit_function(&func, &mut summary);
    }
    summary
}

fn audit_function(func: &IrFunc, summary: &mut SretAuditSummary) {
    let sret_param = match bind_sret_param(func.header) {
        SretBinding::NotSret => return,
        SretBinding::Unparseable => {
            summary.findings.push(SretFinding {
                function: func.name.clone(),
                kind: SretFindingKind::UnparseableHeader,
                line: func.header.trim().to_string(),
            });
            return;
        }
        SretBinding::Bound(name) => name,
    };
    summary.returns.note_candidate();

    for (i, line) in func.body.iter().enumerate() {
        let t = strip_comment(line.trim());
        if !(t == "ret void" || t.starts_with("ret void,")) {
            continue;
        }
        summary.returns.note_tracked();
        if !ret_is_covered(&func.body[..i], &sret_param) {
            summary.findings.push(SretFinding {
                function: func.name.clone(),
                kind: SretFindingKind::BareReturn,
                line: line.trim().to_string(),
            });
        }
    }
}

/// Result of binding the sret parameter name from a `define` header.
enum SretBinding {
    /// No `sret(` attribute in the parameter list — not an sret function.
    NotSret,
    /// Mentions `sret(` but the parameter name could not be bound.
    Unparseable,
    Bound(String),
}

/// Find the parameter carrying the `sret(…)` attribute and bind its SSA name
/// (`%0`, `%out`, …) — no positional assumption. Fails closed.
fn bind_sret_param(header: &str) -> SretBinding {
    if !header.contains("sret") {
        return SretBinding::NotSret;
    }
    let Some(params) = param_list(header) else {
        // Mentions sret but the param list can't even be isolated.
        return SretBinding::Unparseable;
    };
    let Some(entry) = split_top_level_commas(params)
        .into_iter()
        .find(|p| p.contains("sret("))
    else {
        // "sret" appeared outside the param list (e.g. in the function
        // name) — not an sret function.
        return SretBinding::NotSret;
    };
    // The parameter name is the last whitespace token and must be a %-name
    // outside the sret(...) type (attribute tokens like `sret(%T)` start
    // with "sret", not "%").
    match entry.split_whitespace().last() {
        Some(name) if name.starts_with('%') => SretBinding::Bound(name.to_string()),
        _ => SretBinding::Unparseable,
    }
}

/// Isolate the parameter list of a `define` header via depth-matched parens
/// (the first `(` opens it; nested `sret({ … })` / `dereferenceable(N)`
/// parens are balanced through).
fn param_list(header: &str) -> Option<&str> {
    let open = header.find('(')?;
    let mut depth = 0i32;
    for (i, c) in header[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&header[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// True when the instruction immediately preceding a `ret void` (skipping
/// blanks, comments, and stopping at the block label) is a store through the
/// sret parameter or a `musttail call` forwarding it.
fn ret_is_covered(preceding: &[&str], sret_param: &str) -> bool {
    for line in preceding.iter().rev() {
        let t = strip_comment(line.trim());
        if t.is_empty() {
            continue;
        }
        // Block label (`case_left19:`, `else10:`) — the ret opens its block.
        if t.split_whitespace()
            .next()
            .is_some_and(|tok| tok.ends_with(':'))
        {
            return false;
        }
        return is_sret_store(t, sret_param) || is_musttail_forward(t, sret_param);
    }
    false
}

/// Strip a trailing `;` comment (labels keep their `; preds = …` handled by
/// the caller via first-token inspection; instruction operands emitted by
/// this compiler never contain `;`).
fn strip_comment(t: &str) -> &str {
    t.find(';').map_or(t, |i| t[..i].trim_end())
}

/// `store <val>, ptr <sret-param>[, align N][, !meta]` — any top-level
/// comma-separated operand equal to `ptr <sret-param>` counts.
fn is_sret_store(t: &str, sret_param: &str) -> bool {
    let Some(operands) = t.strip_prefix("store ") else {
        return false;
    };
    let target = format!("ptr {sret_param}");
    split_top_level_commas(operands)
        .iter()
        .skip(1)
        .any(|p| p.trim() == target)
}

/// A `musttail call` whose argument list carries the sret parameter (the
/// epilogue forwards the out-pointer to the tail callee).
fn is_musttail_forward(t: &str, sret_param: &str) -> bool {
    if !t.contains("musttail call") {
        return false;
    }
    let (Some(open), Some(close)) = (t.find('('), t.rfind(')')) else {
        return false;
    };
    t[open + 1..close]
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '{' | '}'))
        .any(|tok| tok == sret_param)
}
