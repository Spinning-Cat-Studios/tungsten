//! Codegen-driving collector for the per-unit cost census (ADR 8.7.26a §2.1).
//!
//! Elaborates the project, runs **real** LLVM codegen at `jobs = 1` (output
//! discarded), and returns one [`UnitCostRecord`] per codegen unit. Sequential
//! compilation is load-bearing for attribution: with a single worker thread,
//! each unit's thread-local allocation delta and wall time are unpolluted by
//! concurrent units.
//!
//! Lives in `compile/` (not `doctor/`) because it requires the `codegen`
//! feature, mirroring `tco::collect` (ADR 1.7.26b).

use std::path::PathBuf;
use std::sync::Mutex;

use tungsten_bootstrap::driver;
use tungsten_core::diagnostics::unit_cost::UnitCostRecord;

use crate::compile::{find_main_type, per_module, validation, CompileFlags};

/// Elaborate `file`, run stage-1 codegen sequentially, and collect per-unit
/// wall time + allocation volume.
pub(crate) fn collect_unit_costs(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> Result<Vec<UnitCostRecord>, String> {
    let trace_opts = driver::TraceOptions::default();
    let mut project = driver::elaborate_project(file, verbose, max_errors, Some(&trace_opts))
        .map_err(|e| format!("{e}"))?;

    // Same post-elaboration substitution the real compile path applies, so
    // the census measures the types a normal build lowers.
    validation::apply_tyvar_substitutions(
        &mut project.defs,
        &project.type_provenance,
        &project.adt_types,
        verbose,
    );

    let main_ty = find_main_type(file, &project.defs)
        .map_err(|_| "unit-cost census requires a `main` function in the file".to_string())?;

    let flags = CompileFlags {
        emit_llvm: true, // write .ll (discarded) — avoids native obj + linking
        max_errors,
        codegen_jobs: 1, // sequential ⇒ deterministic, unpolluted attribution
        ..Default::default()
    };

    let tmp = tempfile::tempdir().map_err(|e| format!("could not create temp dir: {e}"))?;
    let sink: Mutex<Vec<UnitCostRecord>> = Mutex::new(Vec::new());
    per_module::run_per_module_codegen(
        file,
        &flags,
        &project,
        &main_ty,
        per_module::CodegenOutput {
            dir: tmp.path(),
            musttail_sink: None,
            unit_cost_sink: Some(&sink),
        },
    )?;

    Ok(sink.into_inner().unwrap_or_default())
}
