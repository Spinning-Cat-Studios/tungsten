//! The instantiation-time comparability gate (ADR 1.8.26b D3).
//!
//! `compare<T>` is resolved at instantiation — by the monomorphizer on the
//! codegen path, by the evaluator on the eval path. ADR 29.6.26f's P3 rejection
//! landed on the codegen path only, so on the eval path an unsynthesizable `T`
//! left the application **Stuck**: the enclosing `assert_eq` never executed, the
//! failure flag was never set, and `tungsten test` reported the test `ok`.
//!
//! ## Why this had to be designed rather than merely moved
//!
//! The pre-1.8.26b eval callback decided comparability by `closure.is_empty()`.
//! That predicate cannot express the defects it needed to catch:
//!
//! - **D2** produces a **non-empty** closure. Discovery defines a comparator
//!   under one mangled spelling and the emitted body calls another, so
//!   `is_empty()` is false, the defs are installed, and the unbound recursive
//!   edge goes Stuck exactly as before.
//! - **`Expr`** does not produce a closure at all in finite time — synthesis
//!   ran past 900 s — so no predicate over the result was ever reached.
//!
//! So the gate resolves every `compare_*` symbol the closure's bodies reference
//! against the closure's own definitions, and bounds synthesis. A dangling
//! reference has the same standing as an absent comparator.
//!
//! ## One producer, two consumers
//!
//! [`classify`] is the single decision function. The evaluator calls it to
//! *enforce*, and `doctor check comparable` calls it to *report*, so the
//! diagnostic cannot disagree with what actually happens — the same discipline
//! the guardrail hooks apply to `run_chain`/`explain`.

use std::collections::HashSet;

use tungsten_core::eval::{ComparatorFailure, ComparatorFailureKind};
use tungsten_core::Type;

use crate::elaborate::CoreDef;

use super::context::ComparatorTypes;
use super::discover::{collect_comparator_globals, synth_closure_bounded};
use super::synth;

/// How many comparators synthesis may emit for one type before the gate calls
/// it unsettled.
///
/// Synthesis is not guaranteed to converge: `mu_comparator` materializes an
/// unrolled body as a new type and mangles it, so a μ-chain whose body
/// re-unfolds into ever-larger types spawns a fresh comparator per round.
/// Measured: `Expr` (∏kᵢ ≈ 1.45 M) did not terminate within 900 s. Without a
/// bound the gate cannot report on the very case that motivates it — a
/// diagnostic that wedges is not a diagnostic.
///
/// The value is a *reporting* bound, not a performance budget: every type that
/// compares correctly today settles in far fewer definitions (the whole
/// `Pattern` closure is single digits), so raising or lowering it within an
/// order of magnitude changes nothing except how long a pathological type takes
/// to be reported.
pub const CLOSURE_CAP: usize = 512;

/// A successfully gated comparator: the symbol to call, and the definitions to
/// install so that call resolves.
pub struct GatedClosure {
    pub top_symbol: String,
    pub defs: Vec<CoreDef>,
}

/// Decide whether `ty` has a runnable comparator, and if so produce it.
///
/// The `@`-prefix strip mirrors the codegen callback: a record/ADT arriving as
/// a *type argument* through a generic wrapper carries the Phase-1c prefix,
/// while `record_types` and synthesized symbols are keyed without it.
pub fn classify(ty: &Type, types: &ComparatorTypes) -> Result<GatedClosure, ComparatorFailure> {
    let ty = ty.strip_tyvar_at_prefix();
    let fail = |kind| ComparatorFailure::new(ty.to_string(), kind);

    // 1. Noncomparable by policy — an opaque leaf. Checked first because it is
    //    a statement about the type, not about anything synthesis produced.
    //
    //    The walk can also stop *without* a verdict, when a chain of generic
    //    instantiations exceeds its expansion bound (ADR 1.8.26c). That is not
    //    a statement about the type at all, so it must not be reported as one:
    //    mapping it to `OpaqueLeaf` would say "noncomparable by policy" about
    //    what is really "we stopped looking".
    match synth::check_comparable(&ty, types) {
        Ok(()) => {}
        Err(synth::Noncomparable::Opaque(path)) => {
            return Err(fail(ComparatorFailureKind::OpaqueLeaf { path }))
        }
        Err(synth::Noncomparable::Unsettled(bound)) => {
            return Err(fail(ComparatorFailureKind::LimitExceeded { bound }))
        }
    }

    // 2. Synthesis, bounded. An unsettled closure is reported as such rather
    //    than judged on whatever partial set it had reached.
    let walk = synth_closure_bounded(&ty, types, CLOSURE_CAP);
    if !walk.converged {
        return Err(fail(ComparatorFailureKind::LimitExceeded {
            bound: CLOSURE_CAP,
        }));
    }

    // 3. Nothing synthesizable at all.
    if walk.defs.is_empty() {
        return Err(fail(ComparatorFailureKind::EmptyClosure));
    }

    // 4. Completeness, not emptiness: a body that calls a symbol this closure
    //    never defines is exactly as unrunnable as an absent comparator. The
    //    symbol alone is a poor report — it names the shape that could not be
    //    built, not the reason — so the refused sub-type is re-checked to say
    //    why.
    if let Some(dangling) = first_dangling_reference(&walk.defs) {
        let cause = walk
            .refused_under(&dangling)
            .and_then(|refused| synth::check_comparable(refused, types).err())
            .map(|why| why.to_string());
        return Err(fail(ComparatorFailureKind::IncompleteClosure {
            dangling,
            cause,
        }));
    }

    Ok(GatedClosure {
        top_symbol: super::mangling::comparator_symbol(&ty),
        defs: walk.defs,
    })
}

/// The lexicographically first `compare_*` symbol the closure calls but does
/// not define.
///
/// Sorted rather than "whichever the set iterates first" so the diagnostic is
/// reproducible across runs — a `HashSet` seeds differently per instance, and a
/// gate whose message changes between two identical invocations is a gate
/// people learn to distrust.
pub(crate) fn first_dangling_reference(defs: &[CoreDef]) -> Option<String> {
    let defined: HashSet<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    let mut referenced: HashSet<String> = HashSet::new();
    for def in defs {
        collect_comparator_globals(&def.term.term, &mut referenced);
    }
    let mut dangling: Vec<String> = referenced
        .into_iter()
        .filter(|s| !defined.contains(s.as_str()))
        .collect();
    dangling.sort();
    dangling.into_iter().next()
}

#[cfg(test)]
mod tests;
