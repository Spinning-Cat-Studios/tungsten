//! `tungsten doctor check ir merge-truncation` — flag the 1.7.26e §6.6
//! phi-poisoning signature in emitted LLVM IR (ADR 2.7.26b T6).
//!
//! The §6.6 miscompile typed an ADT-match merge phi from a dead musttail arm's
//! `i1` dummy; `cast_to_type` then "unified" the real `{ptr, ptr}` result
//! through a **1-byte memcpy**, reconstructing garbage. This lint scans each
//! function for `call … @memcpy(ptr %dst, ptr %src, i64 N)` where `%dst`/`%src`
//! are allocas of aggregate types whose store size exceeds `N` **and** a value
//! loaded from `%dst` feeds a `phi` — precisely the truncating merge shape.
//!
//! **Honest about its reach (ADR 2.7.26b §2.6).** This is a heuristic text
//! backstop *behind* the `cast_to_type` shrinking-aggregate hard error (T1),
//! not a sound alias analysis. Supported shape only: the memcpy operands are
//! **direct alloca operands** (single-hop — no GEP/bitcast/provenance chasing,
//! no DataLayout sizing beyond the alloca'd aggregate's declared store size),
//! and the loaded result reaches a `phi` through at most one intervening load.
//! Forms outside that shape (opaque-pointer operands with no local alloca,
//! multi-hop provenance, lifetime-intrinsic interleaving) are NOT matched. If
//! emitted IR later routes these memcpys through GEP/bitcast, this lint must
//! be reimplemented over parsed LLVM IR with the module DataLayout rather than
//! silently under-matching.
//!
//! Exit codes (stable, for CI): 0 = clean, 1 = matches found, 2 = bad input.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::ExitCode;

use crate::doctor::checks::ir_checks::corpus::{
    reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts,
};
use crate::doctor::checks::ir_checks::textparse::{parse_define_name, split_top_level_commas};

#[cfg(test)]
mod tests;

/// One matched truncating-merge memcpy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationMatch {
    pub function: String,
    /// The offending memcpy line (trimmed).
    pub line: String,
    pub copy_size: u64,
    /// The aggregate type being truncated (dst or src side).
    pub aggregate_type: String,
    pub aggregate_size: u64,
}

/// Per-module scan result (machine-readable counts, ADR 2.7.26b T6/R11).
///
/// `facts.candidates` counts functions scanned and `facts.tracked` the
/// alloca/load/phi facts the matcher bound inside them. The reach measure is
/// the *facts*, not memcpy→phi pairs: the compiler's own corpus emits only five
/// memcpy call sites and none has a literal size operand, so counting matched
/// memcpys would make `--strict` fail on healthy IR — the red-gate failure the
/// guard exists to prevent (ADR 28.7.26e D4). What can silently rot is
/// `collect_facts`: if `alloca`/`load`/`phi` lines stop being recognized the
/// matcher matches nothing, forever, quietly.
#[derive(Default)]
pub struct ScanSummary {
    pub matches: Vec<TruncationMatch>,
    pub facts: ReachCounts,
}

impl ScanSummary {
    /// Functions scanned.
    pub fn functions(&self) -> usize {
        self.facts.candidates
    }
}

/// Scan a directory of `.ll` files for the truncating-merge signature.
pub fn cmd_check_merge_truncation(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_merge_truncation`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }
    let mut facts = ReachCounts::default();
    let mut matches: Vec<(String, TruncationMatch)> = Vec::new();
    let scan = scan_ll_corpus(dir, |path, text| {
        let summary = scan_ir(text);
        facts.add(summary.facts);
        for m in summary.matches {
            matches.push((path.display().to_string(), m));
        }
    });
    scan.charge_unreadable(&mut facts);

    // Machine-readable summary line (files + functions checked, match count).
    println!(
        "summary: files={} functions={} facts_bound={} matches={}",
        scan.files,
        facts.candidates,
        facts.tracked,
        matches.len()
    );
    for (file, m) in &matches {
        println!(
            "match: file={file} function={} copy_size={} aggregate={} aggregate_size={}",
            m.function, m.copy_size, m.aggregate_type, m.aggregate_size
        );
        println!("  {}", m.line);
    }

    let verdict = AuditVerdict::classify(&scan, facts.is_vacuous(), matches.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => println!(
            "\n⚠ {n} truncating merge memcpy(s) — a merge/phi typed from a dead-arm \
             placeholder truncates real results (ADR 2.7.26b T6; see 1.7.26e §6.6)."
        ),
        AuditVerdict::Clean => {
            println!("✓ no truncating merge memcpys in {}", dir.display());
        }
        _ => {}
    }
    verdict.report_vacuity(
        facts.is_vacuous(),
        "functions were scanned but no alloca/load/phi fact was bound",
    );
    verdict
}

/// Scan one LLVM IR module (text). Pure + unit-testable: no filesystem.
pub fn scan_ir(text: &str) -> ScanSummary {
    let mut summary = ScanSummary::default();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("define ") {
            continue;
        }
        let name = parse_define_name(trimmed);
        let mut body = Vec::new();
        for l in lines.by_ref() {
            if l.trim_start() == "}" {
                break;
            }
            body.push(l);
        }
        summary.facts.note_candidate();
        let facts = collect_facts(&body);
        summary.facts.tracked += facts.bound();
        scan_function(&name, &body, &facts, &mut summary.matches);
    }
    summary
}

/// Per-function facts the matcher consults: alloca types, one-hop loads, and
/// phi incoming tokens.
#[derive(Default)]
struct FunctionFacts<'a> {
    /// alloca-defined SSA names → declared type text.
    allocas: HashMap<&'a str, &'a str>,
    /// loaded SSA name → source pointer (one hop).
    loads: HashMap<&'a str, &'a str>,
    /// Every `%`-token in a phi's incoming list — a deliberate **superset** of
    /// the incoming *values*, since the predecessor labels (`%then`, `%else`)
    /// are `%`-prefixed too.
    ///
    /// That is sound for the only question asked of it — "did a value loaded
    /// from this alloca reach a phi?" is a `contains` test, and a superset can
    /// only make the answer more conservative (it never hides a match). It is
    /// NOT a count of phi operands, so do not read `.len()` as one; the name
    /// says `tokens` rather than `inputs` for exactly that reason.
    phi_tokens: HashSet<&'a str>,
}

impl FunctionFacts<'_> {
    /// How many facts the matcher's parser bound in this body — its reach
    /// (ADR 28.7.26e D4), **summed** across the three kinds.
    ///
    /// The sum is load-bearing: a product would report zero for any body
    /// missing one kind (a merge with no allocas is ordinary IR), and zero
    /// tracked against non-zero candidates is vacuity — so `--strict` would
    /// fail on healthy IR. This measures parser reach, not phi arity; the
    /// `phi_tokens` superset above is fine for that purpose.
    fn bound(&self) -> usize {
        self.allocas.len() + self.loads.len() + self.phi_tokens.len()
    }
}

fn scan_function(
    name: &str,
    body: &[&str],
    facts: &FunctionFacts<'_>,
    out: &mut Vec<TruncationMatch>,
) {
    for line in body {
        let t = line.trim();
        if !(t.contains("@memcpy(") || t.contains("@llvm.memcpy")) {
            continue;
        }
        if let Some(m) = match_truncating_memcpy(name, t, facts) {
            out.push(m);
        }
    }
}

/// One pass over the body collecting allocas, loads, and phi tokens.
fn collect_facts<'a>(body: &[&'a str]) -> FunctionFacts<'a> {
    let mut facts = FunctionFacts::default();
    for line in body {
        let t = line.trim();
        let Some(eq) = t.find(" = ") else { continue };
        let (dest, rhs) = (t[..eq].trim(), &t[eq + 3..]);
        if let Some(ty) = rhs.strip_prefix("alloca ") {
            if let Some(ty) = split_top_level_commas(ty).first() {
                facts.allocas.insert(dest, ty.trim());
            }
        } else if rhs.starts_with("load ") {
            record_load(&mut facts, dest, t);
        } else if rhs.starts_with("phi ") {
            record_phi_tokens(&mut facts, rhs);
        }
    }
    facts
}

fn record_load<'a>(facts: &mut FunctionFacts<'a>, dest: &'a str, line: &'a str) {
    if let Some(src) = line.rsplit("ptr ").next() {
        if let Some(src) = src.split([',', ' ']).next() {
            if src.starts_with('%') {
                facts.loads.insert(dest, src);
            }
        }
    }
}

fn record_phi_tokens<'a>(facts: &mut FunctionFacts<'a>, rhs: &'a str) {
    let Some(list) = rhs.find('[') else { return };
    for tok in rhs[list..].split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']')) {
        if tok.starts_with('%') {
            facts.phi_tokens.insert(tok);
        }
    }
}

/// Match one memcpy line against the §6.6 signature (supported shape only:
/// direct-alloca operands, ≤1-hop load into a phi).
fn match_truncating_memcpy(
    name: &str,
    t: &str,
    facts: &FunctionFacts<'_>,
) -> Option<TruncationMatch> {
    // out-of-shape memcpys are unmatched by design (documented reach)
    let (dst, src, n) = parse_memcpy_args(t)?;
    let dst_ty = facts.allocas.get(dst.as_str())?;
    let src_ty = facts.allocas.get(src.as_str())?;

    // The truncated aggregate may sit on either side (§6.6 has both:
    // {ptr,ptr}→i1 in the value arm, i1→{ptr,ptr} at the merge).
    let (agg_ty, agg_size) = [dst_ty, src_ty]
        .into_iter()
        .filter(|ty| is_aggregate(ty))
        .filter_map(|ty| approx_store_size(ty).map(|sz| (*ty, sz)))
        .find(|(_, sz)| *sz > n)?;

    // The loaded result must feed a phi (through at most one load).
    let feeds_phi = facts
        .loads
        .iter()
        .any(|(loaded, from)| *from == dst && facts.phi_tokens.contains(loaded));
    if !feeds_phi {
        return None;
    }

    Some(TruncationMatch {
        function: name.to_string(),
        line: t.to_string(),
        copy_size: n,
        aggregate_type: agg_ty.to_string(),
        aggregate_size: agg_size,
    })
}

/// Parse `(ptr %dst, ptr %src, i64 N …)` from a memcpy call line.
fn parse_memcpy_args(call: &str) -> Option<(String, String, u64)> {
    let open = call.find('(')?;
    let close = call.rfind(')')?;
    let args: Vec<&str> = split_top_level_commas(&call[open + 1..close]);
    if args.len() < 3 {
        return None;
    }
    let ssa = |s: &str| -> Option<String> {
        s.split_whitespace()
            .find(|tok| tok.starts_with('%'))
            .map(str::to_string)
    };
    let dst = ssa(args[0])?;
    let src = ssa(args[1])?;
    let n = args[2].split_whitespace().last()?.parse::<u64>().ok()?;
    Some((dst, src, n))
}

fn is_aggregate(ty: &str) -> bool {
    let t = ty.trim();
    t.starts_with('{') || t.starts_with('[')
}

/// Approximate store size in bytes of a type string — natural alignment,
/// primitives + nested structs/arrays. `None` for shapes we don't model
/// (vector types, named types): those memcpys are unmatched by design.
pub(crate) fn approx_store_size(ty: &str) -> Option<u64> {
    Some(size_align(ty.trim())?.0)
}

/// (size, align) of a type string.
fn size_align(ty: &str) -> Option<(u64, u64)> {
    let t = ty.trim();
    if let Some(inner) = t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut offset = 0u64;
        let mut max_align = 1u64;
        for field in split_top_level_commas(inner) {
            let (sz, al) = size_align(field)?;
            offset = offset.div_ceil(al) * al + sz;
            max_align = max_align.max(al);
        }
        let size = offset.div_ceil(max_align) * max_align;
        return Some((size, max_align));
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let (count, elem) = inner.split_once(" x ")?;
        let count: u64 = count.trim().parse().ok()?;
        let (sz, al) = size_align(elem)?;
        let stride = sz.div_ceil(al) * al;
        return Some((count * stride, al));
    }
    match t {
        "ptr" => Some((8, 8)),
        "float" => Some((4, 4)),
        "double" => Some((8, 8)),
        "half" => Some((2, 2)),
        _ => {
            let bits: u64 = t.strip_prefix('i')?.parse().ok()?;
            let bytes = bits.div_ceil(8);
            Some((bytes, bytes.clamp(1, 8)))
        }
    }
}
