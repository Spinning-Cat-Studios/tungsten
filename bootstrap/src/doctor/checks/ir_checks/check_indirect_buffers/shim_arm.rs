//! The `$direct` shim arm (ADR 17.7.26e §2.2, invariant I4): the shim side of
//! the `noalias` obligation — the buffers it allocates must be **single-routed**
//! into the `$direct_mt` callee, never stored as a value, returned, or handed to
//! any other call.
//!
//! Candidate detection is deliberately **independent of the buffer alloca
//! names**: a shim is recognized by its attributed call into a `$direct_mt`
//! entry, while the buffers themselves are recognized by the stable identifiers
//! `shim.rs::fill_shim_slots` emits (`sret_buf`, `indirect_buf.<i>`). A rename
//! in `shim.rs` therefore leaves candidates > 0 with nothing tracked — a
//! vacuous pass the strict guard turns into a failure (ADR 2.7.26b T5a) rather
//! than a silent green.

use std::collections::HashSet;

use super::parse::Func;
use super::track::{
    call_has_buffer_slot, derives_address, is_direct_mt_call, is_permitted_intrinsic,
    load_pointer_operand, mentions_tracked, store_value_operand, track_derived_addresses,
};
use super::{AuditSummary, BufferFinding, FindingKind};

/// Alloca-name prefix of the shim's sret out-buffer (`shim.rs`).
const SRET_BUF_PREFIX: &str = "%sret_buf";
/// Alloca-name prefix of a shim's per-source-param indirect buffer (`shim.rs`).
const INDIRECT_BUF_PREFIX: &str = "%indirect_buf.";

/// Audit one `$direct` shim definition. `$direct_mt` definitions are the callee
/// arm's business and are skipped here.
pub(super) fn audit_shim(func: &Func, summary: &mut AuditSummary) {
    if func.name.ends_with("$direct_mt") {
        return;
    }
    let is_shim = func
        .body
        .iter()
        .any(|l| is_direct_mt_call(l) && call_has_buffer_slot(l));
    if !is_shim {
        return;
    }
    summary.shim.candidates += 1;

    let (roots, sret_roots) = buffer_allocas(&func.body);
    summary.shim.tracked += roots.len();

    let tracked = track_derived_addresses(&func.body, &roots);
    let sret_tracked = track_derived_addresses(&func.body, &sret_roots);
    for line in &func.body {
        let t = line.trim();
        if mentions_tracked(t, &tracked) && !shim_use_permitted(t, &tracked, &sret_tracked) {
            summary.findings.push(BufferFinding {
                function: func.name.clone(),
                kind: FindingKind::ShimBufferEscape,
                line: t.to_string(),
            });
        }
    }
}

/// The I4 allowlist: the only uses a shim buffer address may appear in.
fn shim_use_permitted(t: &str, tracked: &HashSet<String>, sret_tracked: &HashSet<String>) -> bool {
    if let Some((dest, rhs)) = t.split_once(" = ") {
        // The defining alloca, and any address derived from a tracked address.
        // Permitted only when the line *defines* a tracked address — merely
        // mentioning one somewhere in an alloca/GEP does not earn the pass.
        if rhs.starts_with("alloca") || derives_address(rhs) {
            return tracked.contains(dest.trim());
        }
        // The sret read-back: loading THROUGH the sret buffer after the call.
        // The same load through an `indirect_buf.*` does not break `noalias`
        // (the callee's activation has ended) but does break the shim shape, so
        // it is conservatively a finding.
        if rhs.starts_with("load ") {
            return load_pointer_operand(t).is_some_and(|p| sret_tracked.contains(&p));
        }
    }
    // The fill store — writing THROUGH the buffer. Storing the address itself
    // as a *value* is the escape this excludes.
    if t.starts_with("store ") {
        return store_value_operand(t).is_some_and(|v| !mentions_tracked(v, tracked));
    }
    // Handing the buffer to the `$direct_mt` callee is the whole point; the
    // lifetime/debug/memory intrinsics neither retain it nor outlive the call.
    if t.contains("call") {
        return is_direct_mt_call(t) || is_permitted_intrinsic(t);
    }
    false
}

/// The shim's buffer allocas, by the stable names `shim.rs::fill_shim_slots`
/// emits: all of them, and the sret out-buffer alone (its post-call read-back
/// load is permitted, an indirect buffer's is not).
fn buffer_allocas(body: &[&str]) -> (HashSet<String>, HashSet<String>) {
    let mut all = HashSet::new();
    let mut sret = HashSet::new();
    for line in body {
        let t = line.trim();
        let Some((dest, rhs)) = t.split_once(" = ") else {
            continue;
        };
        let dest = dest.trim();
        if !rhs.starts_with("alloca") {
            continue;
        }
        if dest.starts_with(SRET_BUF_PREFIX) {
            sret.insert(dest.to_string());
            all.insert(dest.to_string());
        } else if dest.starts_with(INDIRECT_BUF_PREFIX) {
            all.insert(dest.to_string());
        }
    }
    (all, sret)
}
