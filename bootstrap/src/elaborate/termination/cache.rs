//! What an elaboration-cache entry has to remember about admission
//! (ADR 29.6.26e §2.5).
//!
//! Split from the adapter because it answers a different question. `mod.rs`
//! asks "what does the engine conclude about these terms"; this asks "what
//! survives when there are no terms" — a module served from the signature
//! cache contributes no `CoreDef`s, so its verdict and its place in the taint
//! graph have to be carried rather than recomputed.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tungsten_core::terms::termination::{CarriedDefs, FailureReason, OccurrenceGraph};

use super::{admission_errors, TerminationInput};
use crate::elaborate::{CoreDef, DefTerminationMeta, ElabError};

/// Everything a cache entry has to remember about a module's admission
/// (ADR 29.6.26e §2.5).
///
/// A module served from the elaboration cache contributes no `CoreDef`s, so
/// without this its rejections would silently disappear on the second run and
/// its `#[partial]` definitions would drop out of the taint graph. Terms are
/// *not* stored: descent was decided when the module was fresh and a value SCC
/// never spans modules, so only the names and edges taint needs are kept.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachedTermination {
    /// This module's rejections, rendered, each with whether it is
    /// proof-relevant — enough to re-decide gating at a different enforcement
    /// level than the one that wrote the entry.
    pub failures: Vec<(ElabError, bool)>,
    /// Definition → the definitions it mentions.
    pub mentions: Vec<(String, Vec<String>)>,
    /// Which of them are annotated `#[partial]`.
    pub partial: Vec<String>,
    /// Which of them are proof-relevant.
    pub proofs: Vec<String>,
}

impl CachedTermination {
    /// Compute a module's cacheable termination facts from its own output.
    #[must_use]
    pub fn from_module(defs: &[CoreDef], meta: &HashMap<String, DefTerminationMeta>) -> Self {
        let input = TerminationInput::from_meta(meta);
        let report = input.check(defs);
        let graph =
            OccurrenceGraph::build(defs.iter().map(|def| (def.name.as_str(), &def.term.term)));
        CachedTermination {
            failures: report
                .failures
                .iter()
                .zip(admission_errors(&report, defs))
                .map(|(failure, error)| {
                    let proof_relevant = input.is_proof(&failure.function)
                        || matches!(failure.reason, FailureReason::PartialInProof { .. });
                    (error, proof_relevant)
                })
                .collect(),
            mentions: defs
                .iter()
                .map(|def| {
                    (
                        def.name.clone(),
                        graph.mentions(&def.name).into_iter().collect(),
                    )
                })
                .collect(),
            partial: meta
                .iter()
                .filter(|(_, entry)| entry.attrs.partial)
                .map(|(name, _)| name.clone())
                .collect(),
            proofs: meta
                .iter()
                .filter(|(_, entry)| entry.is_proof)
                .map(|(name, _)| name.clone())
                .collect(),
        }
    }

    /// Fold another module's facts into this accumulation.
    pub fn absorb(&mut self, other: CachedTermination) {
        self.failures.extend(other.failures);
        self.mentions.extend(other.mentions);
        self.partial.extend(other.partial);
        self.proofs.extend(other.proofs);
    }

    /// Whether nothing was carried.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
            && self.mentions.is_empty()
            && self.partial.is_empty()
            && self.proofs.is_empty()
    }

    /// The engine's view of these definitions.
    #[must_use]
    pub fn as_carried(&self) -> CarriedDefs {
        CarriedDefs {
            mentions: self
                .mentions
                .iter()
                .map(|(name, reached)| (name.clone(), reached.iter().cloned().collect()))
                .collect(),
            partial: self.partial.iter().cloned().collect(),
            proofs: self.proofs.iter().cloned().collect(),
        }
    }
}
