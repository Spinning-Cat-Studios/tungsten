//! Multi-module project elaboration entry points.
//!
//! Houses [`elaborate_project`] (the `compile`/`check` entry) and its live
//! inspector variant [`elaborate_project_with_inspector`] (ADR 21.7.26j).
//! Extracted from `driver::mod` as a file-size paydown; the shared pipeline
//! lives in [`elaborate_project_inner`].
//!
//! The inspector variant, after Body Elaboration, seeds a fresh whole-project
//! `Elaborator` from the accumulated exports and hands it — as a
//! [`ProjectNormalizer`] — to a caller closure, so `doctor check type
//! normalization-consistency` can re-normalize `App(name, args)` against the
//! real project type environment (the §2.1 `encode(fresh) ≡ₙ stored` invariant)
//! even for multi-module trees, which standalone re-elaboration cannot resolve.

use std::path::Path;

use tungsten_core::Context;

// `ProjectNormalizer` lives in `elaborate` (it needs the `Elaborator`'s private
// `env`); the driver constructs and re-exports it (ADR 21.7.26j).
pub use crate::elaborate::ProjectNormalizer;

use super::{
    finalize_codegen_units, full_output_cache_for_compile, per_module, pipeline, prepare_project,
    render_diagnostics_with_source_map, set_max_errors, PipelineError, ProjectOutput, TraceOptions,
};

/// Elaborate a multi-module project, returning the compiled definitions.
///
/// The entry point for `compile`/`check`: unlike `run_file_with_options`, this
/// returns the elaborated definitions (plus record/ADT/alias metadata and the
/// source map) rather than checking or evaluating them. See
/// [`elaborate_project_with_inspector`] for the live-normalization variant.
pub fn elaborate_project(
    path: &Path,
    verbose: bool,
    max_errors: usize,
    trace: Option<&TraceOptions>,
) -> Result<ProjectOutput, PipelineError> {
    elaborate_project_inner(path, verbose, max_errors, trace, None)
}

/// Elaborate a project and, after Body Elaboration, hand a fresh whole-project
/// [`ProjectNormalizer`] to `inspector` before returning the output
/// (ADR 21.7.26j).
///
/// The inspector runs synchronously while the seeded `Elaborator` is alive, so
/// nothing escapes its lifetime. On an elaboration failure the inspector is
/// never called and `Err` is returned — the caller falls back to the cross-run
/// comparison.
pub fn elaborate_project_with_inspector(
    path: &Path,
    verbose: bool,
    max_errors: usize,
    inspector: &mut dyn FnMut(&ProjectNormalizer),
) -> Result<ProjectOutput, PipelineError> {
    elaborate_project_inner(path, verbose, max_errors, None, Some(inspector))
}

/// Live-elaborator inspection hook (ADR 21.7.26j): seed a fresh whole-project
/// `Elaborator` from the Phase-B exports — carrying this run's stored Phase-1e
/// encodings — and hand it to the inspector. The per-module oracle
/// (ADR 22.7.26b) additionally re-runs collection per module to harvest
/// source-fresh Phase-1e encodings — the faithful comparand for records and
/// generic instantiations that `normalize_for_comparison` cannot reproduce.
/// All of it is inspector-only work: the compile path never pays for it.
fn run_normalization_inspector(
    inspect: &mut dyn FnMut(&ProjectNormalizer),
    module_tree: &super::modules::ParsedModule,
    build: &pipeline::BuildCtx<'_>,
    tree_output: &per_module::ModuleTreeOutput,
) {
    let fresh_per_module = per_module::fresh_encodings::per_module_fresh_encodings(
        module_tree,
        build,
        &tree_output.exports,
    );
    let mut ctx = Context::new();
    let normalizer = ProjectNormalizer::seeded(
        &mut ctx,
        &tree_output.exports,
        tree_output.elab.encoded_types.clone(),
        fresh_per_module,
    );
    inspect(&normalizer);
}

/// Shared pipeline for [`elaborate_project`] and
/// [`elaborate_project_with_inspector`]. The only difference is the optional
/// post-Phase-B inspector hook.
fn elaborate_project_inner(
    path: &Path,
    verbose: bool,
    max_errors: usize,
    trace: Option<&TraceOptions>,
    inspector: Option<&mut dyn FnMut(&ProjectNormalizer)>,
) -> Result<ProjectOutput, PipelineError> {
    // Set max_errors for this run
    set_max_errors(max_errors);

    let cache = full_output_cache_for_compile(path, verbose);

    // Parse and prepare the project
    let prepared = prepare_project(path, verbose, cache.as_ref())?;

    // Elaborate per-module with two-phase approach (ADR 5.5.26c)
    let trace_opts = trace.cloned().unwrap_or_default();
    let build = pipeline::BuildCtx {
        cache: cache.as_ref(),
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };
    let tree_output = match per_module::elaborate_module_tree(
        &prepared.module_tree,
        path,
        verbose,
        &build,
        &trace_opts,
    ) {
        Ok(output) => output,
        Err(elab_errors) => {
            let filename = path.to_string_lossy();
            render_diagnostics_with_source_map(
                &prepared.source,
                &filename,
                &build.source_map,
                &elab_errors,
                &[],
            );
            return Err(PipelineError::ElabFailed("elaboration errors".to_string()));
        }
    };

    if let Some(inspect) = inspector {
        run_normalization_inspector(inspect, &prepared.module_tree, &build, &tree_output);
    }

    let output = tree_output.elab;

    // Render any warnings (non-fatal)
    if !output.warnings.is_empty() {
        let filename = path.to_string_lossy();
        render_diagnostics_with_source_map(
            &prepared.source,
            &filename,
            &build.source_map,
            &[],
            &output.warnings,
        );
    }

    if verbose {
        eprintln!("Elaborated {} definition(s)", output.defs.len());
    }

    // Strip @-prefixed TyVars at the elaboration→codegen boundary (ADR 10.5.26d P7).
    // These are an elaboration-internal convention that must not leak downstream.
    let defs: Vec<_> = output
        .defs
        .into_iter()
        .map(|d| d.strip_at_prefixes())
        .collect();
    let comparator_types = crate::comparator::ComparatorTypes::new(
        output.record_types.clone(),
        &output.encoded_types,
        &output.type_provenance,
        output.adt_types.clone(),
        &output.mutual_recursion_groups,
    );
    let codegen_units = finalize_codegen_units(tree_output.module_defs, verbose, &comparator_types);

    Ok(ProjectOutput {
        defs,
        codegen_units,
        record_types: output.record_types,
        adt_types: output.adt_types,
        type_aliases: output.type_aliases,
        type_provenance: output.type_provenance,
        source_map: build.source_map,
        encoded_types: output.encoded_types,
        mutual_recursion_groups: output.mutual_recursion_groups,
        type_visibilities: output.type_visibilities,
        record_field_visibilities: output.record_field_visibilities,
        value_import_targets: tree_output.module_import_targets,
        termination_meta: output.termination_meta,
    })
}
