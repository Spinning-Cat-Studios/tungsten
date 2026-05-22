//! Module exports and phase-check elaboration entry points.

use super::{CollectedElaborator, ElabOutput};
use crate::ast::SourceFile;
use crate::elaborate::env::{ConstructorInfo, TypeDef, ValueDef};
use crate::elaborate::error::ElabError;
use crate::elaborate::Elaborator;
use serde::{Deserialize, Serialize};
use tungsten_core::{Context, Type};

/// Exports from a single module's elaboration (ADR 5.5.26b §3).
///
/// After elaborating a module, these exports are injected into subsequent
/// modules' environments so they can resolve cross-module references.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleExports {
    /// Type definitions (ADTs, aliases, records) — excludes stubs.
    pub types: Vec<(String, TypeDef)>,
    /// Value definitions (functions, theorems, axioms).
    pub values: Vec<(String, ValueDef)>,
    /// Constructor info entries (constructor → parent type mapping).
    pub constructors: Vec<(String, ConstructorInfo)>,
}

/// Run the collection pass with module info and pre-injected exports (ADR 5.5.26b §3).
///
/// Like `collect_definitions_with_modules`, but before running collection,
/// injects type/value/constructor definitions from previously-elaborated modules.
/// This allows cross-module references to resolve to real definitions rather than stubs.
pub fn collect_definitions_with_exports<'a>(
    file: &SourceFile,
    core_ctx: &'a mut Context,
    module_info: crate::driver::modules::ModuleInfo,
    prior_exports: &ModuleExports,
) -> Result<CollectedElaborator<'a>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);

    // Populate module registry (creates stubs for all known types)
    elaborator.env.populate_module_info(module_info);

    // Inject real definitions from prior modules (overwrites stubs)
    for (name, def) in &prior_exports.types {
        elaborator.env.types.insert(name.clone(), def.clone());
    }
    for (name, def) in &prior_exports.values {
        elaborator.env.values.insert(name.clone(), def.clone());
    }
    for (name, info) in &prior_exports.constructors {
        if elaborator.env.trace_ctor_registration {
            eprintln!(
                "[ctor-reg] seed {} (parent={}, index={}) via seed_ctors_into_env",
                name, info.type_name, info.index
            );
        }
        elaborator
            .env
            .constructors
            .insert(name.clone(), info.clone());
    }

    // Allow value overwrites since Phase A stubs may be pre-registered (ADR 5.5.26c)
    elaborator.allow_value_overwrite = true;

    elaborator.run_collection_pass(file)?;
    Ok(CollectedElaborator {
        elaborator,
        file: file.clone(),
    })
}

/// Legacy result type for backwards compatibility.
#[derive(Debug)]
pub struct CollectionResult {
    /// All type definitions collected.
    pub types: Vec<(String, TypeDef)>,
    /// All value signatures collected.
    pub values: Vec<(String, Type)>,
}

/// Elaborate with phase invariant checking enabled (ADR 20.4.26e).
///
/// Runs the full elaboration pipeline with invariant checks inserted
/// at each phase boundary. Returns the check results regardless of
/// whether elaboration itself succeeded.
pub fn elaborate_with_phase_checks(
    file: &SourceFile,
    core_ctx: &mut Context,
) -> (
    Vec<super::super::phase_checks::PhaseCheckResult>,
    Result<ElabOutput, Vec<ElabError>>,
) {
    let mut elaborator = Elaborator::new(core_ctx);
    elaborator.set_check_phase_invariants(true);

    let result = match elaborator.elaborate_file(file) {
        Ok(defs) => Ok(ElabOutput {
            defs,
            warnings: std::mem::take(&mut elaborator.warnings),
            record_types: elaborator.get_record_types(),
            adt_types: elaborator.get_adt_types(),
            type_aliases: elaborator.get_type_aliases(),
            type_provenance: std::mem::take(&mut elaborator.type_provenance),
            encoded_types: elaborator.get_encoded_types(),
            mutual_recursion_groups: elaborator.get_mutual_recursion_groups(),
            type_visibilities: elaborator.get_type_visibilities(),
            record_field_visibilities: elaborator.get_record_field_visibilities(),
        }),
        Err(errors) => Err(errors),
    };

    let phase_results = elaborator.take_phase_invariant_results();
    (phase_results, result)
}
