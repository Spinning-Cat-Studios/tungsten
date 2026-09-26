//! Public types and entry-point functions for the elaborator.
//!
//! Contains the types returned from elaboration (`CoreDef`, `ElabOutput`,
//! `TypeProvenance`, etc.) and the top-level entry points (`elaborate`,
//! `elaborate_with_warnings`, `collect_definitions`, …).

use serde::{Deserialize, Serialize};

use crate::ast::{Item, SourceFile, Visibility};
use crate::span::Span;
use tungsten_core::terms::{SpannedTerm, TermSpan};
use tungsten_core::Type;

use super::env::{
    self, Constructor, ConstructorInfo, ModuleContents, ModulePath, TypeDef, TypeDefKind, ValueDef,
};
use super::error::ElabError;
use super::termination::CachedTermination;
use super::{Elaborator, ExpectedContext};

mod entry;
pub use entry::{
    collect_definitions, collect_definitions_with_modules, elaborate, elaborate_with_warnings,
    elaborate_with_warnings_full,
};

/// A fully elaborated definition ready for the Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreDef {
    /// The name of this definition
    pub name: String,
    /// The type of this definition
    pub ty: Type,
    /// The term (value/proof) of this definition, wrapped with source span
    /// (ADR 17.4.26a §3.1 — SpannedTerm wrapper, Approach B)
    pub term: SpannedTerm,
    /// Source span for error reporting
    pub span: Span,
}

impl CoreDef {
    /// Strip `@`-prefixed TyVars from this definition's type and term (ADR 10.5.26d P7).
    ///
    /// `@`-prefixed TyVars are an elaboration-internal convention (Type-Body Collection cross-references,
    /// ADR 13.4.26c §2). They must not leak past the elaboration→codegen boundary. This
    /// method strips them in both the type signature and all type annotations in the term body.
    #[must_use]
    pub fn strip_at_prefixes(mut self) -> Self {
        self.ty = self.ty.strip_tyvar_at_prefix();

        // Collect @-prefixed type vars from the term and build a substitution
        // map that strips the @ prefix: @Foo → TyVar("Foo").
        let at_vars: std::collections::HashMap<String, Type> = self
            .term
            .term
            .free_type_vars()
            .into_iter()
            .filter(|v| v.starts_with('@'))
            .map(|v| (v.clone(), Type::TyVar(v[1..].to_string())))
            .collect();
        if !at_vars.is_empty() {
            self.term = SpannedTerm {
                term: self.term.term.substitute_type_vars(&at_vars),
                span: self.term.span,
            };
        }

        self
    }
}

/// What the termination gate needs to know about a definition beyond its term
/// (ADR 29.6.26e).
///
/// Carried alongside `CoreDef` rather than on it, so the many stub and
/// cache-reconstruction sites that build a `CoreDef` are untouched, and so an
/// imported definition arrives with its annotations by the same route as a
/// freshly elaborated one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefTerminationMeta {
    /// `#[partial]` / `#[decreasing(arg)]`, as written.
    pub attrs: crate::ast::TerminationAttrs,
    /// Whether the definition is a theorem, lemma or axiom — the proof-relevant
    /// side of the taint boundary.
    pub is_proof: bool,
}

/// Origin information for a μ-binder created during ADT encoding (ADR 13.4.26c §3).
///
/// Records which ADT, with which type arguments and constructors, produced a
/// given μ-binder. This is advisory metadata — not preserved through structural
/// rewrites — consumed read-only by downstream tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdtOrigin {
    /// Name of the source ADT (e.g., "List")
    pub adt_name: String,
    /// Concrete type arguments at the encoding site (e.g., [String])
    pub type_args: Vec<Type>,
    /// Constructor names (e.g., ["Nil", "Cons"])
    pub constructors: Vec<String>,
}

/// Map from μ-binder names to their ADT origins (ADR 13.4.26c §3).
///
/// Built during `encode_adt_type_impl` and threaded through `ElabOutput` to
/// downstream consumers (`--dump-ir`, `--dump-encoding`, `extract_type_param_substitution`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeProvenance {
    /// Maps μ-binder name (e.g., "α_List") to its ADT origin.
    pub mu_origins: std::collections::HashMap<String, AdtOrigin>,
}

/// Result of elaboration including warnings.
///
/// `Default` yields the all-empty output. Prefer `ElabOutput { field, ..Default::default() }`
/// at stub/reconstruction sites (cache reconstruction, cache-miss returns) so
/// adding a field there costs nothing; the genuine elaboration builders spell
/// out every field on purpose, so a new field is a compile error that forces a
/// deliberate value (ADR 12.7.26a retrospective — struct-field blast radius).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ElabOutput {
    /// The elaborated definitions (empty if there were errors)
    pub defs: Vec<CoreDef>,
    /// Non-fatal warnings encountered during elaboration
    pub warnings: Vec<ElabError>,
    /// Record type definitions: name -> fields.
    /// Used by codegen to expand `TyVar("RecordName")` to structural product types.
    pub record_types: std::collections::HashMap<String, Vec<(String, Type)>>,
    /// ADT type definitions: name -> (params, constructors).
    /// Used by codegen to expand `Type::App("Name", args)` to sum/mu types.
    pub adt_types: std::collections::HashMap<String, (Vec<String>, Vec<env::Constructor>)>,
    /// Type alias definitions: name -> (params, target type).
    /// Used by `info types` / `info encoding` for display.
    pub type_aliases: std::collections::HashMap<String, (Vec<String>, Type)>,
    /// Type provenance: μ-binder → ADT origin (ADR 13.4.26c §3).
    pub type_provenance: TypeProvenance,
    /// Cached type encodings from Encoding Finalization (ADR 20.4.26c).
    /// Maps type name → encoded Type for non-parameterized types.
    pub encoded_types: std::collections::HashMap<String, Type>,
    /// Mutual recursion groups from Recursion Grouping SCC (ADR 20.4.26c).
    /// Maps type name → full SCC group members. Only for SCCs of size > 1.
    pub mutual_recursion_groups: std::collections::HashMap<String, Vec<String>>,
    /// Parent type visibilities (ADR 14.5.26c).
    /// Maps type name → declared visibility. Used by `info type members visibility`.
    pub type_visibilities: std::collections::HashMap<String, crate::ast::Visibility>,
    /// Per-field visibility overrides for record types (ADR 14.5.26c).
    /// Maps record name → per-field visibility (None = inherit parent).
    pub record_field_visibilities:
        std::collections::HashMap<String, Vec<Option<crate::ast::Visibility>>>,
    /// Per-definition termination metadata (ADR 29.6.26e): definition name →
    /// its annotations and proof-relevance. Read by the admission gate.
    pub termination_meta: std::collections::HashMap<String, DefTerminationMeta>,
    /// Termination facts replayed from an elaboration-cache hit (ADR 29.6.26e).
    /// Empty on the fresh path, where the gate has the terms and recomputes them.
    pub carried_termination: super::termination::CachedTermination,
    /// This module's value import targets (ADR 12.7.26a §2.1): original name →
    /// canonical defining module. Populated by `elaborate_with_exports`
    /// (per-module Body Elaboration, where the flat import map is exactly this module's
    /// imports); empty for combined-AST entry points.
    pub value_import_targets: env::ValueImportTargets,
}

#[cfg(test)]
mod test_helpers;

/// Result type for elaboration
pub type ElabResult<T> = Result<T, ElabError>;

/// Elaborate a parsed source file to Core definitions.
/// Result of running the collection pass.
///
/// This represents an elaborator that has completed the collection pass
/// and is ready to either:
/// - Compute a types hash for cache lookup
/// - Continue to the elaboration pass if cache miss
pub struct CollectedElaborator<'a> {
    pub(super) elaborator: Elaborator<'a>,
    pub(super) file: SourceFile,
}

impl<'a> CollectedElaborator<'a> {
    /// Set the trace target for --trace-types (ADR 13.4.26c §5).
    pub fn set_trace_target(&mut self, target: Option<String>) {
        self.elaborator.set_trace_target(target);
    }

    /// Set the trace target for --trace-encoding (ADR 18.4.26h §3).
    pub fn set_trace_encoding(&mut self, target: Option<String>) {
        self.elaborator.set_trace_encoding(target);
    }

    /// Set the trace target for --trace-normalization (ADR 20.4.26c).
    pub fn set_trace_normalization(&mut self, target: Option<String>) {
        self.elaborator.set_trace_normalization(target);
    }

    /// Set the elaboration mode (ADR 5.5.26a).
    pub fn set_elab_mode(&mut self, mode: super::ElabMode) {
        self.elaborator.elab_mode = mode;
    }

    /// Apply all trace and mode options from a `TraceOptions` bundle.
    pub fn apply_trace_options(&mut self, trace: &crate::driver::output::TraceOptions) {
        self.set_trace_target(trace.trace_types.clone());
        // Only override the env-seeded default (ADR 22.7.26d) when the CLI flag
        // is actually present; an absent `--trace-encoding` must not wipe a
        // `TUNGSTEN_TRACE_ENCODING` target.
        if let Some(target) = &trace.trace_encoding {
            self.set_trace_encoding(Some(target.clone()));
        }
        self.set_trace_normalization(trace.trace_normalization.clone());
        self.set_elab_mode(trace.elab_mode);
        self.elaborator.trace_ctor_registration = trace.trace_ctor_registration;
        self.elaborator.env.trace_ctor_registration = trace.trace_ctor_registration;
    }

    /// Whether the collection pass recorded (and deferred) any errors
    /// (ADR 14.8.26g D2).
    ///
    /// The pass no longer short-circuits, so `collect_definitions*` returning
    /// `Ok` is not evidence of a clean collection. A caller that never runs
    /// Pass 2 — and therefore never reaches the drain in `elaborate` /
    /// `elaborate_with_exports` — must consult this before trusting the
    /// collected environment (the D2a audit).
    pub fn has_collection_errors(&self) -> bool {
        !self.elaborator.errors.is_empty()
    }

    /// Drain the deferred collection-pass errors (ADR 14.8.26g D2, D4).
    ///
    /// For callers that report on the collection pass itself rather than
    /// continuing to Pass 2. Draining here means the same errors cannot also
    /// flow out of a later `elaborate*` call — each error is consumed exactly
    /// once (D2a).
    pub fn take_collection_errors(&mut self) -> Vec<ElabError> {
        std::mem::take(&mut self.elaborator.errors)
    }

    /// The Phase-1e type encodings this collection pass produced
    /// (`name → encoded Type`, non-parameterized types only). Used by the
    /// per-module normalization oracle (ADR 22.7.26b) to harvest a
    /// source-fresh comparand without running body elaboration.
    pub fn phase1e_encodings(&self) -> std::collections::HashMap<String, Type> {
        self.elaborator.get_encoded_types()
    }

    /// Get the collected types for computing a types hash.
    pub fn types_for_hash(&self) -> Vec<(String, TypeDef)> {
        self.elaborator.env.export_types_for_hash()
    }

    /// Get the collected value signatures for computing a types hash.
    pub fn value_signatures_for_hash(&self) -> Vec<(String, Type)> {
        self.elaborator.env.export_value_signatures_for_hash()
    }

    /// Extract value exports from the collection pass without running Phase 2.
    ///
    /// Used by Signature Collection (ADR 5.5.26c) to collect global function signatures
    /// before per-module body elaboration. Only extracts values — types and
    /// constructors come from Stub Registration — except a stub the pass
    /// POISONED (ADR 15.8.26d): that one must replace the Stub Registration
    /// placeholder, or every dependent module keeps building against the
    /// placeholder's unresolved field types and re-diagnoses the fault.
    pub fn extract_value_exports(self) -> ModuleExports {
        ModuleExports {
            types: self
                .elaborator
                .env
                .types
                .iter()
                .filter(|(_, def)| def.is_signature_collection_export())
                .map(|(name, def)| (name.clone(), def.clone()))
                .collect(),
            values: self
                .elaborator
                .env
                .values
                .iter()
                .map(|(name, def)| (name.clone(), def.clone()))
                .collect(),
            constructors: self
                .elaborator
                .env
                .constructors
                .iter()
                .map(|(name, info)| (name.clone(), info.clone()))
                .collect(),
        }
    }

    /// Continue to the elaboration pass after a cache miss.
    ///
    /// This consumes the CollectedElaborator and produces the final CoreDefs.
    pub fn elaborate(mut self) -> Result<ElabOutput, Vec<ElabError>> {
        // Pass 2: Elaborate each definition
        let defs = self.elaborator.run_body_pass(&self.file.items);

        if self.elaborator.errors.is_empty() {
            Ok(ElabOutput {
                defs,
                warnings: std::mem::take(&mut self.elaborator.warnings),
                record_types: self.elaborator.get_record_types(),
                adt_types: self.elaborator.get_adt_types(),
                type_aliases: self.elaborator.get_type_aliases(),
                type_provenance: std::mem::take(&mut self.elaborator.type_provenance),
                encoded_types: self.elaborator.get_encoded_types(),
                mutual_recursion_groups: self.elaborator.get_mutual_recursion_groups(),
                type_visibilities: self.elaborator.get_type_visibilities(),
                record_field_visibilities: self.elaborator.get_record_field_visibilities(),
                termination_meta: std::mem::take(&mut self.elaborator.termination_meta),
                carried_termination: CachedTermination::default(),
                // Combined-AST entry point: the flat import map mixes every
                // module's imports, so no per-module table exists here.
                value_import_targets: env::ValueImportTargets::new(),
            })
        } else {
            Err(std::mem::take(&mut self.elaborator.errors))
        }
    }

    /// Elaborate and also return exports for per-module injection (ADR 5.5.26b §3).
    ///
    /// Like `elaborate()`, but also extracts the type/value/constructor definitions
    /// from the elaborator's environment for injection into subsequent modules.
    pub fn elaborate_with_exports(mut self) -> Result<(ElabOutput, ModuleExports), Vec<ElabError>> {
        // Pass 2: Elaborate each definition
        let defs = self.elaborator.run_body_pass(&self.file.items);

        if self.elaborator.errors.is_empty() {
            // Extract exports from env (non-stub types, all values, all constructors)
            let exports = ModuleExports {
                types: self
                    .elaborator
                    .env
                    .types
                    .iter()
                    .filter(|(_, def)| !matches!(def.kind, TypeDefKind::Stub))
                    .map(|(name, def)| (name.clone(), def.clone()))
                    .collect(),
                values: self
                    .elaborator
                    .env
                    .values
                    .iter()
                    .map(|(name, def)| (name.clone(), def.clone()))
                    .collect(),
                constructors: self
                    .elaborator
                    .env
                    .constructors
                    .iter()
                    .map(|(name, info)| (name.clone(), info.clone()))
                    .collect(),
            };

            Ok((
                ElabOutput {
                    defs,
                    warnings: std::mem::take(&mut self.elaborator.warnings),
                    record_types: self.elaborator.get_record_types(),
                    adt_types: self.elaborator.get_adt_types(),
                    type_aliases: self.elaborator.get_type_aliases(),
                    type_provenance: std::mem::take(&mut self.elaborator.type_provenance),
                    encoded_types: self.elaborator.get_encoded_types(),
                    mutual_recursion_groups: self.elaborator.get_mutual_recursion_groups(),
                    type_visibilities: self.elaborator.get_type_visibilities(),
                    record_field_visibilities: self.elaborator.get_record_field_visibilities(),
                    termination_meta: std::mem::take(&mut self.elaborator.termination_meta),
                    carried_termination: CachedTermination::default(),
                    // Per-module Body Elaboration: the flat import map is exactly this
                    // module's processed `use` items (ADR 12.7.26a §2.1).
                    value_import_targets: self.elaborator.env.extract_value_import_targets(),
                },
                exports,
            ))
        } else {
            Err(std::mem::take(&mut self.elaborator.errors))
        }
    }
}

mod body_pass;
mod exports;
mod poison;
mod tests;
pub use exports::{
    collect_definitions_for_signature_collection, collect_definitions_with_exports,
    elaborate_with_phase_checks, CollectionResult, ModuleExports,
};
pub use poison::first_poisoned_export;
