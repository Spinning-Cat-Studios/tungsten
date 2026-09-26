//! Monomorphization state for the code generator (extracted from `codegen/mod.rs`).

use std::collections::{HashMap, HashSet};

use super::comparator::ComparatorSynth;

/// State for monomorphization of polymorphic functions.
pub(crate) struct MonomorphState {
    /// Already monomorphized function instances.
    pub(crate) instances: HashMap<(String, String), String>,
    /// Functions currently being monomorphized (cycle prevention).
    pub(crate) in_progress: HashSet<(String, String)>,
    /// When true, the single-owner mono pipeline (ADR 8.5.26g) is active.
    /// Ad-hoc per-unit monomorphization must not generate fresh instances;
    /// all mono symbols must come from the pre-seeded ownership map.
    pub(crate) mono_map_active: bool,
    /// Lazy comparator synthesis callback (ADR 29.6.26f P6′ step 2).
    pub(crate) comparator_synth: Option<ComparatorSynth>,
    /// Comparator symbols already emitted in *this* unit (per-unit dedup).
    pub(crate) emitted_comparators: HashSet<String>,
}

impl MonomorphState {
    pub(crate) fn new() -> Self {
        Self {
            instances: HashMap::new(),
            in_progress: HashSet::new(),
            mono_map_active: false,
            comparator_synth: None,
            emitted_comparators: HashSet::new(),
        }
    }
}
