//! Arena allocation attribution for the self-compiled heap profile (ADR 2.7.26a §3.4).
//!
//! The class-tagged `--alloc-profile` wrapper only sees allocations made by
//! *codegen-emitted* code (`mu_alloc` / `env_alloc` / …). Everything the
//! arena retains — every `Type`/`Term`/`Context` deep-cloned into a slot by
//! the FFI constructors — is allocated by the Rust allocator and was
//! previously unattributed (the "gap between profiled bytes and RSS").
//! Run #1 measured that gap at ~90% of RSS, so this module closes it:
//! cumulative deep-byte counters accumulated at allocation time (gated on
//! profiler activity), reported per phase/module marker alongside VmRSS.
//!
//! Post-ADR-2.7.26a-§4, types and terms are handle-children NODE arenas —
//! their retention counters accumulate per-node heap only (see
//! `types::nodes`/`terms::nodes`). The deep owned-tree walkers below remain
//! the sizing authority for what still owns full trees: `Context` bindings
//! (`deep_ctx_bytes` → `deep_type_bytes` → `deep_term_bytes` via `Eq`).

use crate::context::{Binding, Context};
use crate::terms::Term;
use crate::types::Type;

use super::Arena;

/// Cumulative arena retention counters (deep bytes per class), accumulated
/// on each `alloc_*` when the allocation profiler is active. Bytes are
/// *deep* estimates: the heap memory owned by the stored value (child boxes,
/// string/vec buffers), not counting the slot itself inside the arena `Vec`
/// (reported separately via `Vec::capacity` in [`marker_line`]).
#[derive(Default)]
pub(crate) struct ArenaRetentionStats {
    pub types: u64,
    pub terms: u64,
    pub ctxs: u64,
}

/// Heap bytes owned by a `Type` tree (excluding the root's inline size).
pub(crate) fn deep_type_bytes(ty: &Type) -> u64 {
    let type_size = size_of::<Type>() as u64;
    match ty {
        Type::Bool
        | Type::Nat
        | Type::Int
        | Type::Unit
        | Type::Void
        | Type::Prop
        | Type::String
        | Type::Error => 0,
        Type::TyVar(v) => v.capacity() as u64,
        Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
            2 * type_size + deep_type_bytes(a) + deep_type_bytes(b)
        }
        Type::Forall(v, t) | Type::Mu(v, t) => v.capacity() as u64 + type_size + deep_type_bytes(t),
        Type::Eq(t, lhs, rhs) => {
            type_size
                + deep_type_bytes(t)
                + 2 * size_of::<Term>() as u64
                + deep_term_bytes(lhs)
                + deep_term_bytes(rhs)
        }
        Type::Ptr(t) | Type::Ref(t) => type_size + deep_type_bytes(t),
        Type::App(name, args) => {
            name.capacity() as u64
                + (args.capacity() as u64) * type_size
                + args.iter().map(deep_type_bytes).sum::<u64>()
        }
        Type::Adt(name, type_args, variants) => {
            name.capacity() as u64
                + (type_args.capacity() as u64) * type_size
                + type_args.iter().map(deep_type_bytes).sum::<u64>()
                + (variants.capacity() as u64) * size_of::<(String, Type)>() as u64
                + variants
                    .iter()
                    .map(|(n, t)| n.capacity() as u64 + deep_type_bytes(t))
                    .sum::<u64>()
        }
    }
}

/// Heap bytes owned by a `Term` tree (excluding the root's inline size).
///
/// Flat dispatcher over the ~58 `Term` variants; arms delegate to the
/// child-summing helpers below so each shape is written once.
///
/// **Sibling walker:** `Term::for_each_subterm` (`terms/traversal.rs`)
/// enumerates the same child structure. When a variant is added or its
/// children change, update BOTH matches — the
/// `walkers_agree_on_children_for_every_variant` test below cross-checks
/// this walker against the traversal and fails if a boxed child is missed
/// here (a miss silently undercounts arena retention).
pub(crate) fn deep_term_bytes(term: &Term) -> u64 {
    match term {
        Term::True
        | Term::False
        | Term::Unit
        | Term::Zero
        | Term::NatLit(_)
        | Term::IntLit(_)
        | Term::Sorry => 0,
        Term::Var(v) | Term::Global(v) => v.capacity() as u64,
        Term::StringLit(s) => s.capacity() as u64,
        Term::Lambda(v, ty, t) | Term::Fix(v, ty, t) => {
            v.capacity() as u64 + deep_type_bytes(ty) + boxed_terms(&[t])
        }
        Term::App(a, b)
        | Term::NatAdd(a, b)
        | Term::NatSub(a, b)
        | Term::NatMul(a, b)
        | Term::NatDiv(a, b)
        | Term::NatMod(a, b)
        | Term::NatEq(a, b)
        | Term::NatLt(a, b)
        | Term::NatLe(a, b)
        | Term::NatGt(a, b)
        | Term::NatGe(a, b)
        | Term::IntBin(_, a, b)
        | Term::BoolAnd(a, b)
        | Term::BoolOr(a, b)
        | Term::StrConcat(a, b)
        | Term::StrEq(a, b)
        | Term::StrCharAt(a, b)
        | Term::Pair(a, b)
        | Term::RefSet(a, b) => boxed_terms(&[a, b]),
        Term::Let(v, ty, a, b) => v.capacity() as u64 + deep_type_bytes(ty) + boxed_terms(&[a, b]),
        Term::If(c, t, e) | Term::StrSubstring(c, t, e) => boxed_terms(&[c, t, e]),
        Term::Absurd(ty, t)
        | Term::Inl(ty, t)
        | Term::Inr(ty, t)
        | Term::Refl(ty, t)
        | Term::Fold(ty, t)
        | Term::Unfold(ty, t) => deep_type_bytes(ty) + boxed_terms(&[t]),
        Term::Succ(t)
        | Term::BoolNot(t)
        | Term::StrLen(t)
        | Term::IntNeg(t)
        | Term::NatToInt(t)
        | Term::IntToNat(t)
        | Term::Fst(t)
        | Term::Snd(t)
        | Term::RefNew(t)
        | Term::RefGet(t)
        | Term::Return(t) => boxed_terms(&[t]),
        Term::NatRec(ty, a, b, c) | Term::NatInd(ty, a, b, c) => {
            deep_type_bytes(ty) + boxed_terms(&[a, b, c])
        }
        Term::Case(s, v1, a, v2, b) => {
            v1.capacity() as u64 + v2.capacity() as u64 + boxed_terms(&[s, a, b])
        }
        Term::TyAbs(v, t) => v.capacity() as u64 + boxed_terms(&[t]),
        Term::TyApp(t, ty) | Term::Annot(t, ty) => deep_type_bytes(ty) + boxed_terms(&[t]),
        Term::Subst(ty1, ty2, a, b) => {
            deep_type_bytes(ty1) + deep_type_bytes(ty2) + boxed_terms(&[a, b])
        }
        Term::ExternCall(name, args) => {
            name.capacity() as u64
                + (args.capacity() as u64) * size_of::<Term>() as u64
                + args.iter().map(deep_term_bytes).sum::<u64>()
        }
        Term::AdtConstruct(ty, _, t) => deep_type_bytes(ty) + boxed_terms(&[t]),
        Term::AdtMatch(s, arms) => {
            boxed_terms(&[s])
                + (arms.capacity() as u64) * size_of::<(usize, String, Box<Term>)>() as u64
                + arms
                    .iter()
                    .map(|(_, v, t)| v.capacity() as u64 + boxed_terms(&[t]))
                    .sum::<u64>()
        }
        Term::Spanned(t, _) => boxed_terms(&[t]),
    }
}

/// Heap bytes owned by boxed term children: each box's inline `Term` slot
/// plus the child's own deep bytes.
fn boxed_terms(children: &[&Term]) -> u64 {
    children
        .iter()
        .map(|t| size_of::<Term>() as u64 + deep_term_bytes(t))
        .sum()
}

/// Heap bytes owned by a `Context` (its bindings vector + each binding).
pub(crate) fn deep_ctx_bytes(ctx: &Context) -> u64 {
    (ctx.bindings().capacity() as u64) * size_of::<Binding>() as u64
        + ctx
            .bindings()
            .iter()
            .map(|b| match b {
                Binding::Term(v, ty) => v.capacity() as u64 + deep_type_bytes(ty),
                Binding::TypeVar(v) => v.capacity() as u64,
            })
            .sum::<u64>()
}

/// Current process resident set size in KiB (Linux only; `None` elsewhere).
///
/// Pure I/O shim over [`parse_vm_rss_kb`]. Its `-> None` mutant is
/// platform-equivalent on macOS (no `/proc`), so only a Linux mutation run
/// can kill it — the parsing logic below is what tests pin down.
fn vm_rss_kb() -> Option<u64> {
    parse_vm_rss_kb(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// Extract the `VmRSS:` value (KiB) from `/proc/<pid>/status` text.
fn parse_vm_rss_kb(status_text: &str) -> Option<u64> {
    let line = status_text.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// One-line arena retention snapshot printed under each phase/module marker.
pub(crate) fn marker_line(arena: &Arena) -> String {
    marker_line_with_rss(arena, vm_rss_kb())
}

/// Testable core of [`marker_line`]: the RSS reading is injected so the
/// KiB→MiB arithmetic is verifiable off-Linux (where `/proc` is absent).
pub(crate) fn marker_line_with_rss(arena: &Arena, rss_kb: Option<u64>) -> String {
    let type_size = size_of::<super::types::nodes::TypeNode>() as u64;
    let term_size = size_of::<super::terms::nodes::TermNode>() as u64;
    let ctx_size = size_of::<Context>() as u64;
    let slab_bytes = (arena.types.capacity() as u64) * type_size
        + (arena.terms.capacity() as u64) * term_size
        + (arena.ctxs.capacity() as u64) * ctx_size;
    let rss = match rss_kb {
        Some(kb) => format!("{}", kb / 1024),
        None => "na".to_string(),
    };
    format!(
        "  [arena] types={} deep={}MB terms={} deep={}MB ctxs={} slab={}MB vmrss={}MB",
        arena.types.len(),
        arena.retention.types / (1024 * 1024),
        arena.terms.len(),
        arena.retention.terms / (1024 * 1024),
        arena.ctxs.len(),
        slab_bytes / (1024 * 1024),
        rss,
    )
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod walker_exact_tests;
