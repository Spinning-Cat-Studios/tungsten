//! Parameter-strictness fixpoint over the type graph (ADR 7.8.26e D2).
//!
//! Recursing into `App` arguments at the *current* mode is unsound:
//! `type Fn1<T> = Mk(T -> Nat)` uses `T` in a forbidden position, so
//! `type Bad2 = B(Fn1<Bad2>)` must be rejected even though `Bad2` never sits
//! syntactically left of an arrow. This module computes, for every named type,
//! how each of its parameters is used — the input the walker's three-way
//! argument dispatch reads.

use std::collections::BTreeMap;

use super::defs::PositivityDefs;
use super::lattice::{Mode, Occ};
use super::walker::{Observer, Walk};

/// Per-type parameter strictness: type name → one [`Occ`] per parameter, in
/// declaration order.
pub type ParamOccs = BTreeMap<String, Vec<Occ>>;

/// Compute the least fixed point of parameter strictness over `defs`.
///
/// Seeds every parameter [`Occ::Unused`] and widens under
/// `Unused ⊑ Strict ⊑ Forbidden` until nothing changes. The lattice has height
/// two per parameter, so the number of rounds is bounded by
/// `2 * total_params + 1`; the loop carries that bound explicitly so a future
/// non-monotone edit fails loudly instead of hanging.
///
/// Computed **once per collection pass** over all definitions — the fixpoint is
/// over the whole type graph, and recomputing it per SCC would be quadratic for
/// no gain.
#[must_use]
pub fn param_occurrences(defs: &PositivityDefs) -> ParamOccs {
    let mut occs: ParamOccs = defs
        .iter()
        .map(|(name, def)| (name.clone(), vec![Occ::Unused; def.params.len()]))
        .collect();

    let total_params: usize = occs.values().map(Vec::len).sum();

    for _ in 0..max_widening_rounds(total_params) {
        let mut widened = false;
        for (name, def) in defs.iter() {
            let mut sink = ParamSink {
                seen: vec![Occ::Unused; def.params.len()],
            };
            for ctor in &def.ctors {
                for (_, ty) in &ctor.fields {
                    Walk::new(defs, &occs, &def.params, &mut sink).field(ty);
                }
            }
            let current = occs.get_mut(name).expect("seeded above");
            for (slot, found) in current.iter_mut().zip(&sink.seen) {
                let joined = slot.join(*found);
                if joined != *slot {
                    *slot = joined;
                    widened = true;
                }
            }
        }
        if !widened {
            return occs;
        }
    }
    occs
}

/// How many widening rounds can be needed before the fixpoint is reached.
///
/// The lattice has height two per parameter (`Unused ⊑ Strict ⊑ Forbidden`), so
/// at most `2 * total_params` rounds can widen anything; the `+ 1` is the round
/// that observes no change and returns. Carried explicitly so a future
/// non-monotone edit fails loudly instead of hanging.
fn max_widening_rounds(total_params: usize) -> usize {
    2 * total_params + 1
}

/// Records the strongest mode each enclosing-definition parameter was reached at.
struct ParamSink {
    seen: Vec<Occ>,
}

impl Observer for ParamSink {
    fn param(&mut self, index: usize, mode: Mode) {
        if let Some(slot) = self.seen.get_mut(index) {
            *slot = slot.join(mode.as_occ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::max_widening_rounds;

    #[test]
    fn the_round_bound_covers_two_widenings_per_parameter_plus_a_settling_round() {
        assert_eq!(max_widening_rounds(0), 1, "one round to observe no change");
        assert_eq!(max_widening_rounds(1), 3);
        assert_eq!(max_widening_rounds(5), 11);
    }
}
