//! The `$direct_mt` callee arm: R10 forwarding, the R4 derived-address escape
//! audit (ADR 1.7.26e §2.5/§2.6), and the I5 tail-edge distinctness assert
//! (ADR 17.7.26e §2.2).

use std::collections::HashSet;

use super::parse::Func;
use super::track::{
    buffer_slot_args, is_permitted_intrinsic, ptr_call_args, track_derived_addresses,
};
use super::{AuditSummary, BufferFinding, FindingKind};

/// Audit one `$direct_mt` definition. Non-`$direct_mt` functions are the shim
/// arm's business and are skipped here.
pub(super) fn audit_callee(func: &Func, summary: &mut AuditSummary) {
    if !func.name.ends_with("$direct_mt") {
        return;
    }

    if func.has_buffer_param() {
        summary.callee.candidates += 1;
    }

    let allocas = alloca_names(&func.body);

    // (1) Self-musttail: its ptr args must be forwarded params, never allocas
    //     (R10), and the buffer slots must be pairwise distinct (I5). Collect
    //     the **buffer** params forwarded unchanged — the sret + indirect-param
    //     pointers, identified by their canonical slot attributes — so the
    //     escape audit (2) targets the bytes `noalias` actually covers and not
    //     a flat recursive-ADT `ptr` argument.
    let mut forwarded: HashSet<String> = HashSet::new();
    let self_musttail = format!("@\"{}\"", func.name);
    for line in &func.body {
        let t = line.trim();
        if !(t.contains("musttail call") && t.contains(&self_musttail)) {
            continue;
        }
        for arg in ptr_call_args(t) {
            if allocas.contains(arg.as_str()) {
                summary
                    .findings
                    .push(finding(func, FindingKind::ForwardedAlloca, t));
            } else if func.buffer_params.contains(&arg) {
                forwarded.insert(arg);
            }
        }
        if let Some(dup) = duplicated_buffer_slot(t) {
            summary.findings.push(finding(
                func,
                FindingKind::TailEdgeAliasedForward,
                &format!("{t}    [{dup} forwarded into two buffer slots]"),
            ));
        }
    }
    summary.callee.tracked += forwarded.len();

    // (2) Derived-address escape audit (R4): track addresses derived from the
    //     forwarded buffers through gep/bitcast/addrspacecast to a fixpoint.
    let tracked = track_derived_addresses(&func.body, &forwarded);
    for line in &func.body {
        let t = line.trim();
        if let Some(f) = check_escape(t, &tracked, &self_musttail) {
            summary
                .findings
                .push(finding(func, FindingKind::PointerEscape, &f));
        }
    }
}

/// The first pointer forwarded into two distinct **buffer slots** of one
/// `musttail` edge (I5). Aliasing two `noalias` slots on the next activation is
/// exactly the miscompile every other gate would still call green.
fn duplicated_buffer_slot(musttail: &str) -> Option<String> {
    let mut seen: HashSet<String> = HashSet::new();
    buffer_slot_args(musttail)
        .into_iter()
        .find(|arg| !seen.insert(arg.clone()))
}

/// alloca-defined SSA names in a function body.
fn alloca_names<'a>(body: &[&'a str]) -> HashSet<&'a str> {
    body.iter()
        .filter_map(|l| {
            let t = l.trim_start();
            let eq = t.find(" = ")?;
            if t[eq..].contains("alloca ") {
                Some(t[..eq].trim())
            } else {
                None
            }
        })
        .collect()
}

/// Check one instruction for an escape of a tracked address: stored as a
/// value, returned, or passed to a non-forwarding, non-permitted call.
/// Returns the offending line if it escapes.
fn check_escape(t: &str, tracked: &HashSet<String>, self_musttail: &str) -> Option<String> {
    for p in tracked {
        // Stored as a VALUE (`store ptr %p, …`) — writing THROUGH it is fine.
        if t.starts_with(&format!("store ptr {p},")) || t == format!("ret ptr {p}") {
            return Some(t.to_string());
        }
    }
    // A call carrying a tracked pointer: permitted only for the forwarding
    // self-musttail, memory intrinsics, lifetime markers, and debug intrinsics.
    if t.contains("call") {
        let is_forwarding = t.contains("musttail call") && t.contains(self_musttail);
        if !is_forwarding && !is_permitted_intrinsic(t) {
            for arg in ptr_call_args(t) {
                if tracked.contains(&arg) {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

fn finding(func: &Func, kind: FindingKind, line: &str) -> BufferFinding {
    BufferFinding {
        function: func.name.clone(),
        kind,
        line: line.to_string(),
    }
}
