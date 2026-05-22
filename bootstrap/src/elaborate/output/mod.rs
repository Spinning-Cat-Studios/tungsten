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
    /// `@`-prefixed TyVars are an elaboration-internal convention (Phase 1c cross-references,
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
#[derive(Debug, Serialize, Deserialize)]
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
    /// Cached type encodings from Phase 1e (ADR 20.4.26c).
    /// Maps type name → encoded Type for non-parameterized types.
    pub encoded_types: std::collections::HashMap<String, Type>,
    /// Mutual recursion groups from Phase 1c.5 SCC (ADR 20.4.26c).
    /// Maps type name → full SCC group members. Only for SCCs of size > 1.
    pub mutual_recursion_groups: std::collections::HashMap<String, Vec<String>>,
    /// Parent type visibilities (ADR 14.5.26c).
    /// Maps type name → declared visibility. Used by `info type visibility`.
    pub type_visibilities: std::collections::HashMap<String, crate::ast::Visibility>,
    /// Per-field visibility overrides for record types (ADR 14.5.26c).
    /// Maps record name → per-field visibility (None = inherit parent).
    pub record_field_visibilities:
        std::collections::HashMap<String, Vec<Option<crate::ast::Visibility>>>,
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
        self.set_trace_encoding(trace.trace_encoding.clone());
        self.set_trace_normalization(trace.trace_normalization.clone());
        self.set_elab_mode(trace.elab_mode);
        self.elaborator.trace_ctor_registration = trace.trace_ctor_registration;
        self.elaborator.env.trace_ctor_registration = trace.trace_ctor_registration;
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
    /// Used by Phase A.5 (ADR 5.5.26c) to collect global function signatures
    /// before per-module body elaboration. Only extracts values — types and
    /// constructors come from Phase A.
    pub fn extract_value_exports(self) -> ModuleExports {
        ModuleExports {
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
        }
    }

    /// Continue to the elaboration pass after a cache miss.
    ///
    /// This consumes the CollectedElaborator and produces the final CoreDefs.
    pub fn elaborate(mut self) -> Result<ElabOutput, Vec<ElabError>> {
        // Pass 2: Elaborate each definition
        let mut defs = Vec::new();
        for item in &self.file.items {
            match self.elaborator.elaborate_item(item) {
                Ok(Some(def)) => defs.push(def),
                Ok(None) => {} // Type definitions don't produce CoreDefs
                Err(e) => self.elaborator.record_error(e), // Use record_error to attach file path
            }
        }

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
        let mut defs = Vec::new();
        for item in &self.file.items {
            match self.elaborator.elaborate_item(item) {
                Ok(Some(def)) => defs.push(def),
                Ok(None) => {}
                Err(e) => self.elaborator.record_error(e),
            }
        }

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
                },
                exports,
            ))
        } else {
            Err(std::mem::take(&mut self.elaborator.errors))
        }
    }
}

mod exports;
mod tests;
pub use exports::{
    collect_definitions_with_exports, elaborate_with_phase_checks, CollectionResult, ModuleExports,
};
