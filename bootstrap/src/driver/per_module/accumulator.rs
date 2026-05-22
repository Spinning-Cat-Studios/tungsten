//! Accumulator types for per-module elaboration (ADR 5.5.26c).
//!
//! Groups accumulated state during the per-module elaboration loop into
//! separate structs to keep `mod.rs` within complexity limits.

use std::collections::HashMap;

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
    /// Def count from cache hits (bodies not re-elaborated, but count preserved for reporting).
    pub(super) cached_def_count: usize,
    /// Whether Phase A.5 global collection succeeded (ADR 13.5.26g §2.2).
    /// When false, E0001 errors in Phase B are annotated with a hint.
    pub(super) phase_a5_ok: bool,
}

impl ModuleTreeAccumulator {
    pub(super) fn new() -> Self {
        Self {
            defs: Vec::new(),
            warnings: Vec::new(),
            type_meta: AccumulatedTypeMeta::new(),
            exports: ModuleExports::default(),
            module_defs: Vec::new(),
            cached_def_count: 0,
            phase_a5_ok: true,
        }
    }

    pub(super) fn merge_output(&mut self, mut output: ElabOutput) {
        self.defs.extend(std::mem::take(&mut output.defs));
        self.warnings.extend(std::mem::take(&mut output.warnings));
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
        self.module_defs.extend(other.module_defs);
        self.cached_def_count += other.cached_def_count;
        // Merge exports from the worker (new entries added by worker's subtree)
        self.merge_exports(other.exports);
    }
}
