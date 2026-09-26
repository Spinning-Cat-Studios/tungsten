//! The termination mirror's protocol, as a state machine (ADR 19.8.26d §2.2).
//!
//! Separate from the externs so the protocol is testable without a C boundary,
//! and separate from [`super::wire`] so the *shape* of a definition stream and
//! the *rendering* of a failure cannot be confused for one another.
//!
//! ## Why three streams and not one
//!
//! P0 measured the one-stream design — every definition's `Term` and `Type`
//! materialized and held live — at **+587 MiB**, against D1's +250 MiB
//! ceiling, of which 91.6% was terms held for an analysis that needs a fifth
//! of them. So the stream reduces instead:
//!
//! ```text
//! reset ( note_item )* ( declare )* ( add_def )* plan ( add_def )* check
//! ```
//!
//! - **declare** fixes the node set. It has to be complete before any mention
//!   set is computed, because `OccurrenceGraph::build_knowing` filters
//!   occurrences against the names it recognises: a definition reduced before
//!   its callees were declared would record edges to none of them.
//! - **add_def** in [`Phase::Reduce`] materializes, reduces to the name sets
//!   taint and the graph need, and **drops** the payload.
//! - **plan** runs Tarjan over those name sets and marks the members of every
//!   recursive component. Descent is the one consumer that needs a real term,
//!   and only for those.
//! - **add_def** in [`Phase::Retain`] materializes the same stream again and
//!   keeps only the planned definitions. Re-streaming is cheaper than a
//!   name-keyed re-lookup over the caller's list, and the peak was already
//!   paid: transient materialization is a high-water mark, not a sum.
//!
//! A recursive group with a `#[partial]` member is planned like any other even
//! though descent skips it, because it still **seeds taint** for its
//! non-annotated members — the arm that makes "every recursive group" the
//! right retention set rather than "every checked group".

use std::collections::{BTreeMap, BTreeSet};

use crate::terms::termination::{
    analyze_with_carried, globals_in_type, is_recursive, tarjan_scc, CarriedDefs, DefRole, DefView,
    OccurrenceGraph, TerminationAnnotation, TerminationFailure,
};
use crate::terms::Term;
use crate::types::Type;

use super::wire::ProtocolError;

/// Which stream the registry is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Names are still being declared and payloads reduced.
    Reduce,
    /// The plan is fixed; payloads are retained if they are in it.
    Retain,
    /// The analysis has run.
    Checked,
}

/// What an item's attributes and kind said about it.
///
/// Only the exceptions are noted, exactly as the bootstrap's
/// `record_termination_meta` does: an absent entry is an unannotated
/// executable definition, which is the overwhelmingly common case.
#[derive(Debug, Clone, Default)]
pub(crate) struct ItemNote {
    pub annotation: TerminationAnnotation,
    pub proof: bool,
}

/// One retained definition, owned for as long as the analysis needs it.
#[derive(Debug)]
struct RetainedDef {
    ty: Type,
    term: Term,
    annotation: TerminationAnnotation,
    role: DefRole,
}

/// The ambient environment the externs drive.
#[derive(Debug, Default)]
pub(crate) struct TerminationRegistry {
    phase: PhaseState,
    /// Every definition name, fixed before any reduction (see module docs).
    declared: BTreeSet<String>,
    /// Per-item attributes and proof-relevance, keyed by name.
    notes: BTreeMap<String, ItemNote>,
    /// name → what it mentions, for every definition. The reduced payload.
    mentions: BTreeMap<String, BTreeSet<String>>,
    /// The recursive-component members, fixed by [`TerminationRegistry::plan`].
    planned: BTreeSet<String>,
    /// The planned definitions' payloads, live only from `plan` to `check`.
    retained: BTreeMap<String, RetainedDef>,
    /// The last analysis's rejections.
    failures: Vec<TerminationFailure>,
}

/// `Default` for [`Phase`] as a named wrapper, so the registry can derive it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhaseState(Phase);

impl Default for PhaseState {
    fn default() -> Self {
        PhaseState(Phase::Reduce)
    }
}

impl TerminationRegistry {
    /// Discard the previous pass entirely — definitions, plan and verdict.
    pub(crate) fn reset(&mut self) {
        *self = TerminationRegistry::default();
    }

    /// The phase the registry is in.
    pub(crate) fn phase(&self) -> Phase {
        self.phase.0
    }

    /// Record one item's attributes and role.
    ///
    /// Last-write-wins, like the positivity registry's `defs` map: body
    /// elaboration can re-enter an item, and overwriting with the newest
    /// reading keeps the environment the best-available one rather than the
    /// first-seen one (R1).
    pub(crate) fn note_item(&mut self, name: &str, note: ItemNote) -> Result<(), ProtocolError> {
        self.expect(Phase::Reduce, "note_item")?;
        self.notes.insert(name.to_string(), note);
        Ok(())
    }

    /// Declare one definition name.
    pub(crate) fn declare(&mut self, name: &str) -> Result<(), ProtocolError> {
        self.expect(Phase::Reduce, "declare")?;
        self.declared.insert(name.to_string());
        Ok(())
    }

    /// Offer one definition's payload, in whichever phase the registry is in.
    ///
    /// A name that was never declared is a protocol error rather than a new
    /// node: the node set is what every mention set was filtered against, and
    /// admitting a late one would give it edges the earlier definitions could
    /// not have recorded to it.
    pub(crate) fn add_def(
        &mut self,
        name: &str,
        ty: Type,
        term: Term,
    ) -> Result<(), ProtocolError> {
        if !self.declared.contains(name) {
            return Err(ProtocolError::UndeclaredDefinition(name.to_string()));
        }
        match self.phase() {
            Phase::Reduce => {
                let reached = self.reduce(name, &ty, &term);
                self.mentions.insert(name.to_string(), reached);
                Ok(())
            }
            Phase::Retain => {
                if self.planned.contains(name) {
                    let note = self.notes.get(name).cloned().unwrap_or_default();
                    self.retained.insert(
                        name.to_string(),
                        RetainedDef {
                            ty,
                            term,
                            annotation: note.annotation,
                            role: if note.proof {
                                DefRole::Proof
                            } else {
                                DefRole::Executable
                            },
                        },
                    );
                }
                Ok(())
            }
            Phase::Checked => Err(ProtocolError::AfterCheck("add_def")),
        }
    }

    /// Reduce one payload to the name sets the graph and taint need.
    ///
    /// The occurrence half goes through the engine's own
    /// `OccurrenceGraph::build_knowing` rather than a hand-rolled walk, so this
    /// side cannot drift from the side that decides the verdict. The type half
    /// is what `MentionIndex::build` adds for an `Eq` witness's embedded terms.
    fn reduce(&self, name: &str, ty: &Type, term: &Term) -> BTreeSet<String> {
        let graph = OccurrenceGraph::build_knowing(std::iter::once((name, term)), &self.declared);
        let mut reached = graph.mentions(name);
        reached.extend(globals_in_type(ty));
        reached
    }

    /// Close the reduction stream and decide what must be retained.
    ///
    /// Returns how many definitions the plan holds, which is how many the
    /// caller's second stream will actually keep.
    ///
    /// **`with_descent` selects which half of the analysis the caller is
    /// mirroring.** Taint needs only the name sets pass 1 already reduced;
    /// descent needs real terms, and it is the caller — not this side — that
    /// knows whether its terms carry what descent reads. The self-hosted
    /// elaborator's do not yet (ADR 19.8.26d §7), so it passes `false` and no
    /// definition is retained at all.
    pub(crate) fn plan(&mut self, with_descent: bool) -> Result<usize, ProtocolError> {
        self.expect(Phase::Reduce, "plan")?;
        if with_descent {
            let adjacency: BTreeMap<String, BTreeSet<String>> = self.mentions.clone();
            for component in tarjan_scc(&adjacency) {
                if is_recursive(&component, &adjacency) {
                    self.planned.extend(component);
                }
            }
        }
        self.phase = PhaseState(Phase::Retain);
        Ok(self.planned.len())
    }

    /// Run the analysis and record its rejections. Returns the count.
    pub(crate) fn check(&mut self) -> Result<usize, ProtocolError> {
        self.expect(Phase::Retain, "check")?;
        let views: BTreeMap<String, DefView<'_>> = self
            .retained
            .iter()
            .map(|(name, def)| {
                (
                    name.clone(),
                    DefView {
                        ty: &def.ty,
                        term: &def.term,
                        annotation: &def.annotation,
                        role: def.role,
                    },
                )
            })
            .collect();
        let report = analyze_with_carried(&views, &self.carried());
        self.failures = report.failures;
        self.phase = PhaseState(Phase::Checked);
        Ok(self.failures.len())
    }

    /// Every definition that was *not* retained, as the engine's carried set.
    ///
    /// The self-hosted compiler has no elaboration cache, so this channel
    /// carries no cache entry — it carries the definitions this seam reduced
    /// instead, which is the same thing from the engine's side: a name, its
    /// edges, and whether it is annotated or proof-relevant.
    fn carried(&self) -> CarriedDefs {
        let mut carried = CarriedDefs::default();
        for (name, reached) in &self.mentions {
            if self.retained.contains_key(name) {
                continue;
            }
            carried.mentions.insert(name.clone(), reached.clone());
            let Some(note) = self.notes.get(name) else {
                continue;
            };
            if note.annotation.partial {
                carried.partial.insert(name.clone());
            }
            if note.proof {
                carried.proofs.insert(name.clone());
            }
        }
        carried
    }

    /// The rejections the last [`TerminationRegistry::check`] found.
    pub(crate) fn failures(&self) -> &[TerminationFailure] {
        &self.failures
    }

    /// Refuse a call made in the wrong phase.
    fn expect(&self, want: Phase, call: &'static str) -> Result<(), ProtocolError> {
        if self.phase() == want {
            Ok(())
        } else {
            Err(ProtocolError::WrongPhase {
                call,
                found: self.phase(),
            })
        }
    }
}
