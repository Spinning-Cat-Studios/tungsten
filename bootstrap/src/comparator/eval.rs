//! Wiring the lazy comparator-synthesis callback into the evaluator
//! (ADR 29.6.26f §T11.2a / P6′).
//!
//! The evaluator (in `tungsten_core`) resolves `__cmp<T>` — the generic `compare`
//! intrinsic — by invoking a callback that synthesizes `compare_T` on demand.
//! `tungsten_core` cannot depend on `bootstrap` (where synthesis lives), so the
//! callback is supplied here and installed on the `EvalEnv`.

use std::collections::HashMap;
use std::rc::Rc;

use tungsten_core::eval::{ComparatorSynth, EvalEnv};
use tungsten_core::{Term, Type};

use super::context::ComparatorTypes;
use super::gate;

/// An `EvalEnv` over `globals` with the lazy `__cmp<T>` synthesis callback
/// installed. `types` lets the callback resolve named record types and
/// μ-cluster members.
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: callers all use the default hasher
pub fn eval_env(globals: HashMap<String, Term>, types: &ComparatorTypes) -> EvalEnv {
    EvalEnv::new(globals).with_comparator_synth(synth_callback(types.clone()))
}

/// The callback: gate `T`, then hand back `(top_symbol, defs)` — or the
/// enumerated reason it cannot be compared (ADR 1.8.26b D3).
///
/// The decision itself lives in [`gate::classify`] so `doctor check comparable`
/// reports on exactly what the evaluator enforces; this function is only the
/// shape adapter between that and `tungsten_core`'s callback type.
fn synth_callback(types: ComparatorTypes) -> ComparatorSynth {
    Rc::new(move |ty: &Type| {
        let gated = gate::classify(ty, &types)?;
        Ok((
            gated.top_symbol,
            gated
                .defs
                .into_iter()
                .map(|d| (d.name, d.term.term))
                .collect(),
        ))
    })
}
