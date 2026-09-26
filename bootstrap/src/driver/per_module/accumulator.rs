//! Accumulator types for per-module elaboration (ADR 5.5.26c).
//!
//! Groups accumulated state during the per-module elaboration loop into
//! separate structs to keep `mod.rs` within complexity limits.

use std::collections::HashMap;

use crate::elaborate::termination::{self, CachedTermination};
use crate::elaborate::{ElabError, ElabOutput, ModuleExports, TypeProvenance};
use tungsten_core::Type;

/// Type-level metadata accumulated across modules.
///
/// Groups the 6 type metadata maps to reduce field count on `ModuleTreeAccumulator`.
pub(super) struct AccumulatedTypeMeta {
    pub(super) record_types: HashMap<String, Vec<(String, Type)>>,
    pub(super) adt_types: HashMap<String, (Vec<String>, Vec<crate::elaborate::Constructor>)>,
    pub(super) type_aliases: HashMap<String, (Vec<String>, Type)>,
    pub(super) type_provenance: TypeProvenance,
    pub(super) encoded_types: HashMap<String, Type>,
    pub(super) mutual_recursion_groups: HashMap<String, Vec<String>>,
    pub(super) type_visibilities: HashMap<String, crate::ast::Visibility>,
    pub(super) record_field_visibilities: HashMap<String, Vec<Option<crate::ast::Visibility>>>,
}

impl AccumulatedTypeMeta {
    pub(super) fn new() -> Self {
        Self {
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_aliases: HashMap::new(),
            type_provenance: TypeProvenance::default(),
            encoded_types: HashMap::new(),
            mutual_recursion_groups: HashMap::new(),
            type_visibilities: HashMap::new(),
            record_field_visibilities: HashMap::new(),
        }
    }

    pub(super) fn merge_from(&mut self, output: &mut ElabOutput) {
        self.record_types
            .extend(std::mem::take(&mut output.record_types));
        self.adt_types.extend(std::mem::take(&mut output.adt_types));
        self.type_aliases
            .extend(std::mem::take(&mut output.type_aliases));
        self.type_provenance
            .mu_origins
            .extend(std::mem::take(&mut output.type_provenance.mu_origins));
        self.encoded_types
            .extend(std::mem::take(&mut output.encoded_types));
        self.mutual_recursion_groups
            .extend(std::mem::take(&mut output.mutual_recursion_groups));
        self.type_visibilities
            .extend(std::mem::take(&mut output.type_visibilities));
        self.record_field_visibilities
            .extend(std::mem::take(&mut output.record_field_visibilities));
    }
}

/// What makes two errors the same fault reported twice: file, span and code.
/// The same identity the display dedup keys on, applied earlier — at error
/// assembly — where a duplicate can still be dropped without corrupting the
/// raw diagnostic count.
fn error_identity(
    error: &ElabError,
) -> (Option<std::path::PathBuf>, crate::span::Span, &'static str) {
    (error.file_path.clone(), error.span, error.kind.code())
}

/// Accumulated state during per-module elaboration.
pub(super) struct ModuleTreeAccumulator {
    pub(super) defs: Vec<crate::elaborate::CoreDef>,
    pub(super) warnings: Vec<ElabError>,
    pub(super) type_meta: AccumulatedTypeMeta,
    pub(super) exports: ModuleExports,
    /// Per-module def groups: (module_path, source_file, defs) for codegen unit partitioning (ADR 7.5.26h)
    pub(super) module_defs: Vec<(
        Vec<String>,
        std::path::PathBuf,
        Vec<crate::elaborate::CoreDef>,
    )>,
    /// Per-module value import targets, keyed by canonical module path
    /// (ADR 12.7.26a §2.1) — codegen's colliding-import resolution input.
    pub(super) module_import_targets:
        std::collections::BTreeMap<Vec<String>, crate::elaborate::ValueImportTargets>,
    /// Termination annotations and proof-relevance per definition
    /// (ADR 29.6.26e). Merged across modules so the gate sees the whole
    /// project's annotations at once, which is what the trusted boundary is.
    pub(super) termination_meta: HashMap<String, crate::elaborate::DefTerminationMeta>,
    /// Termination facts replayed from elaboration-cache hits (ADR 29.6.26e),
    /// accumulated across every module that was served from cache.
    pub(super) carried_termination: CachedTermination,
    /// Def count from cache hits (bodies not re-elaborated, but count preserved for reporting).
    pub(super) cached_def_count: usize,
    /// Whether Signature Collection global collection succeeded (ADR 13.5.26g §2.2).
    /// When false, E0001 errors in Body Elaboration are annotated with a hint.
    pub(super) signature_collection_ok: bool,
    /// How many modules Body Elaboration reached, counting cache hits and
    /// empty modules (ADR 7.8.26d retrospective).
    ///
    /// Compared against the tree's total to report how much of the project a
    /// failing run never examined. Counted at the *top* of `elaborate_self`,
    /// before its early returns, so "reached" means "the walk got here" rather
    /// than "work was done here" — the walk's stopping point is the question.
    pub(super) modules_walked: usize,
    /// Errors accumulated across the module walk (ADR 14.8.26g D1), one group
    /// per failing module, tagged with the module's path. The walk records and
    /// keeps going; the run's verdict is read from here at the end. Tagging by
    /// path (rather than flattening at push time) is what lets the final
    /// report order the groups canonically whatever order the walk — serial
    /// or parallel — produced them in.
    pub(super) module_errors: Vec<(Vec<String>, Vec<ElabError>)>,
    /// Cache entries staged during the walk, committed only if the whole run
    /// succeeded (ADR 14.8.26g D5). A run that had a failure drops them all —
    /// including cleanly-elaborated siblings' — so cache contents never
    /// depend on which module beat a failure to the writer.
    pub(super) pending_cache_writes: Vec<super::cache::PendingCacheWrite>,
    /// Errors from the global (pre-walk) Signature Collection pass
    /// (ADR 14.8.26g D4). Previously dropped after a `warning:` line; now
    /// reported ahead of the per-module groups, so the root cause reads
    /// first. The warning and 13.5.26g's downstream annotation are retained.
    pub(super) pre_walk_errors: Vec<ElabError>,
}

impl ModuleTreeAccumulator {
    /// Record that the walk reached one more module.
    ///
    /// A method rather than a bare `+= 1` at the call site so the increment is
    /// assertable: the call site sits inside `elaborate_self`, which no unit
    /// test drives, and an increment nothing can observe is an increment
    /// nothing can check.
    pub(super) fn note_module_walked(&mut self) {
        self.modules_walked += 1;
    }

    pub(super) fn new() -> Self {
        Self {
            defs: Vec::new(),
            warnings: Vec::new(),
            type_meta: AccumulatedTypeMeta::new(),
            exports: ModuleExports::default(),
            module_defs: Vec::new(),
            module_import_targets: std::collections::BTreeMap::new(),
            termination_meta: HashMap::new(),
            carried_termination: CachedTermination::default(),
            cached_def_count: 0,
            signature_collection_ok: true,
            modules_walked: 0,
            module_errors: Vec::new(),
            pending_cache_writes: Vec::new(),
            pre_walk_errors: Vec::new(),
        }
    }

    /// Whether the run has accumulated any error — a failed module during the
    /// walk (ADR 14.8.26g D1) or a failed pre-walk Signature Collection (D4).
    pub(super) fn has_accumulated_errors(&self) -> bool {
        !self.module_errors.is_empty() || !self.pre_walk_errors.is_empty()
    }

    /// Drain the accumulated errors into the run's diagnostic list: pre-walk
    /// (Signature Collection, D4) errors first — the root cause reads first —
    /// then the per-module groups in canonical walk order (D1).
    ///
    /// The serial walker pushes groups in its own traversal order already; the
    /// parallel walker merges per dependency level, which can differ. Sorting
    /// by the caller-supplied canonical order (the serial traversal, computed
    /// from the tree) makes the diagnostic list identical at every
    /// `thread_count`. A group whose path is somehow absent from the order
    /// sorts last, in push order — reported late rather than lost.
    ///
    /// The global pass elaborates the same items the per-module passes do, so
    /// a fault it recorded usually comes back a second time from its module's
    /// own pass. A pre-walk error whose (file, span, code) some module also
    /// reported is dropped HERE, pre-dedup — D4's guarantee is that the root
    /// cause is in the list, not that it appears twice; leaving the duplicate
    /// for the display dedup would corrupt the raw count that the two-figure
    /// summary (ADR 7.8.26d) exists to keep honest.
    pub(super) fn take_accumulated_errors_ordered(
        &mut self,
        canonical_order: &[Vec<String>],
    ) -> Vec<ElabError> {
        let position: HashMap<&[String], usize> = canonical_order
            .iter()
            .enumerate()
            .map(|(i, path)| (path.as_slice(), i))
            .collect();
        let mut groups = std::mem::take(&mut self.module_errors);
        groups
            .sort_by_key(|(path, _)| position.get(path.as_slice()).copied().unwrap_or(usize::MAX));
        let reported_by_a_module: std::collections::HashSet<_> = groups
            .iter()
            .flat_map(|(_, errors)| errors)
            .map(error_identity)
            .collect();
        let mut errors: Vec<ElabError> = std::mem::take(&mut self.pre_walk_errors)
            .into_iter()
            .filter(|e| !reported_by_a_module.contains(&error_identity(e)))
            .collect();
        errors.extend(groups.into_iter().flat_map(|(_, errors)| errors));
        errors
    }

    pub(super) fn merge_output(&mut self, mut output: ElabOutput) {
        self.defs.extend(std::mem::take(&mut output.defs));
        self.warnings.extend(std::mem::take(&mut output.warnings));
        self.termination_meta
            .extend(std::mem::take(&mut output.termination_meta));
        self.carried_termination
            .absorb(std::mem::take(&mut output.carried_termination));
        self.type_meta.merge_from(&mut output);
    }

    pub(super) fn merge_exports(&mut self, new_exports: ModuleExports) {
        for (name, def) in new_exports.types {
            if let Some(pos) = self.exports.types.iter().position(|(n, _)| n == &name) {
                self.exports.types[pos] = (name, def);
            } else {
                self.exports.types.push((name, def));
            }
        }
        for (name, def) in new_exports.values {
            if let Some(pos) = self.exports.values.iter().position(|(n, _)| n == &name) {
                debug_assert!(
                    self.exports.values[pos].0 == name,
                    "merge_exports: value overwrite name mismatch: existing '{}' vs replacement '{}'",
                    self.exports.values[pos].0,
                    name,
                );
                self.exports.values[pos] = (name, def);
            } else {
                self.exports.values.push((name, def));
            }
        }
        for (name, info) in new_exports.constructors {
            if let Some(pos) = self
                .exports
                .constructors
                .iter()
                .position(|(n, _)| n == &name)
            {
                self.exports.constructors[pos] = (name, info);
            } else {
                self.exports.constructors.push((name, info));
            }
        }
    }

    /// Synthesize the comparators `__compare` emitted but nothing defines, so
    /// they are in the definition set *before* the gate runs (ADR 11.8.26b §2.3).
    ///
    /// They used to be appended to `ElabOutput.defs` after `admit_or_reject`,
    /// which made them a second route into the trusted environment — the exact
    /// hole ADR 29.6.26e §2.5 exists to close. They are structurally recursive
    /// by construction, so this is not a fix for a live unsoundness; it is the
    /// removal of an unchecked path that a later synthesizer change could make
    /// into one.
    ///
    /// Returns how many were added, for the verbose line the callers print.
    pub(super) fn synthesize_comparators(&mut self) -> usize {
        let types = crate::comparator::ComparatorTypes::new(
            self.type_meta.record_types.clone(),
            &self.type_meta.encoded_types,
            &self.type_meta.type_provenance,
            self.type_meta.adt_types.clone(),
            &self.type_meta.mutual_recursion_groups,
        );
        let synthesized =
            crate::comparator::discover::synth_missing_comparators(&self.defs, &types);
        let count = synthesized.len();
        self.defs.extend(synthesized);
        count
    }

    /// Run the termination gate over everything the tree produced
    /// (ADR 29.6.26e §2.5).
    ///
    /// A method on the accumulator because the accumulator *is* the assembled
    /// trusted environment at this point — every module's definitions,
    /// annotations, and whatever the elaboration cache carried forward.
    ///
    /// Failures the current enforcement level does not gate become warnings
    /// rather than being dropped: a definition the checker could not certify is
    /// worth saying out loud even while the corpus is being brought up to the
    /// rule, and both callers already render `warnings`.
    pub(super) fn admit_or_reject(&mut self) -> Result<(), Vec<ElabError>> {
        let carried = self.carried_termination.as_carried();
        let input = termination::TerminationInput::from_meta(&self.termination_meta);
        let report = input.check_with_carried(&self.defs, &carried);
        if termination::has_nothing_to_report(&report, &self.carried_termination) {
            return Ok(());
        }

        let level = termination::enforcement();
        let mut split = input.partition_errors(&report, &self.defs, level);
        // A module served from cache contributed no terms, so its rejections
        // come back rendered rather than recomputed — the alternative is that
        // they are reported once and never again.
        for (error, proof_relevant) in std::mem::take(&mut self.carried_termination.failures) {
            split.push(error, proof_relevant, level);
        }

        self.warnings.extend(split.reported);
        if split.gating.is_empty() {
            Ok(())
        } else {
            Err(split.gating)
        }
    }

    pub(super) fn into_output(self) -> ElabOutput {
        ElabOutput {
            defs: self.defs,
            warnings: self.warnings,
            record_types: self.type_meta.record_types,
            adt_types: self.type_meta.adt_types,
            type_aliases: self.type_meta.type_aliases,
            type_provenance: self.type_meta.type_provenance,
            encoded_types: self.type_meta.encoded_types,
            mutual_recursion_groups: self.type_meta.mutual_recursion_groups,
            type_visibilities: self.type_meta.type_visibilities,
            record_field_visibilities: self.type_meta.record_field_visibilities,
            termination_meta: self.termination_meta,
            carried_termination: self.carried_termination,
            // Per-module import tables ride on `ModuleTreeOutput`
            // (`module_import_targets`), not the merged output.
            value_import_targets: crate::elaborate::ValueImportTargets::new(),
        }
    }

    /// Merge all results from a parallel worker's accumulator (ADR 11.5.26b §P5).
    ///
    /// Unlike `merge_output` + `merge_exports` (which merge a single module's
    /// output), this consumes an entire worker accumulator including its
    /// defs, warnings, type metadata, exports, module_defs, and cached counts.
    pub(super) fn merge_worker(&mut self, other: ModuleTreeAccumulator) {
        self.defs.extend(other.defs);
        self.warnings.extend(other.warnings);
        self.type_meta
            .record_types
            .extend(other.type_meta.record_types);
        self.type_meta.adt_types.extend(other.type_meta.adt_types);
        self.type_meta
            .type_aliases
            .extend(other.type_meta.type_aliases);
        self.type_meta
            .type_provenance
            .mu_origins
            .extend(other.type_meta.type_provenance.mu_origins);
        self.type_meta
            .encoded_types
            .extend(other.type_meta.encoded_types);
        self.type_meta
            .mutual_recursion_groups
            .extend(other.type_meta.mutual_recursion_groups);
        self.type_meta
            .type_visibilities
            .extend(other.type_meta.type_visibilities);
        self.type_meta
            .record_field_visibilities
            .extend(other.type_meta.record_field_visibilities);
        self.termination_meta.extend(other.termination_meta);
        self.carried_termination.absorb(other.carried_termination);
        self.module_defs.extend(other.module_defs);
        self.module_import_targets
            .extend(other.module_import_targets);
        self.cached_def_count += other.cached_def_count;
        self.modules_walked += other.modules_walked;
        self.module_errors.extend(other.module_errors);
        self.pending_cache_writes.extend(other.pending_cache_writes);
        // Merge exports from the worker (new entries added by worker's subtree)
        self.merge_exports(other.exports);
    }
}
