//! Codegen-driving collector for musttail decisions (ADR 1.7.26b).
//!
//! Elaborates the project, runs **real** LLVM codegen sequentially (output
//! discarded), and returns every structured [`MusttailDecision`] emitted at the
//! `check_musttail_abi_safety` gate, alongside a `name → Type` map used to
//! classify each self-recursive function's recursion driver.
//!
//! Lives in `compile/` (not `info/`) because it requires the `codegen` feature,
//! mirroring `check_mono_coverage` (ADR 8.5.26i §2.5).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tungsten_bootstrap::driver;
use tungsten_codegen::MusttailDecision;
use tungsten_core::types::Type;

use crate::compile::{find_main_type, per_module, validation, CompileFlags};

/// Result of a codegen run performed for musttail diagnostics.
pub(crate) struct MusttailRun {
    /// Every structured decision emitted during the run (per tail-call site).
    pub(crate) decisions: Vec<MusttailDecision>,
    /// Source-level function name → its elaborated type (for driver classification).
    /// A `BTreeMap` keeps a deterministic key order — diagnostic-only (never
    /// feeds `.ll` emission), but avoids the codegen-path HashMap-iteration lint.
    pub(crate) fn_types: BTreeMap<String, Type>,
}

/// Elaborate `file`, run codegen, and collect all musttail decisions.
///
/// Runs **sequentially** (`codegen_jobs = 1`) so the decision order is
/// deterministic for snapshots. Emits `.ll` into a temp dir that is dropped on
/// return — we only care about the side-channel decision records.
pub(crate) fn collect_musttail_decisions(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> Result<MusttailRun, String> {
    let trace_opts = driver::TraceOptions::default();
    let mut project = driver::elaborate_project(file, verbose, max_errors, Some(&trace_opts))
        .map_err(|e| format!("{e}"))?;

    // Same post-elaboration substitution the real compile path applies, so the
    // lowered signatures the gate sees match a normal build.
    validation::apply_tyvar_substitutions(
        &mut project.defs,
        &project.type_provenance,
        &project.adt_types,
        verbose,
    );

    let main_ty = find_main_type(file, &project.defs)
        .map_err(|_| "musttail coverage requires a `main` function in the file".to_string())?;

    let fn_types: BTreeMap<String, Type> = project
        .defs
        .iter()
        .map(|d| (d.name.clone(), d.ty.clone()))
        .collect();

    let flags = CompileFlags {
        emit_llvm: true, // write .ll (discarded) — avoids native obj + linking
        max_errors,
        codegen_jobs: 1, // sequential ⇒ deterministic decision ordering
        ..Default::default()
    };

    let tmp = tempfile::tempdir().map_err(|e| format!("could not create temp dir: {e}"))?;
    let sink: Mutex<Vec<MusttailDecision>> = Mutex::new(Vec::new());
    per_module::run_per_module_codegen(
        file,
        &flags,
        &project,
        &main_ty,
        per_module::CodegenOutput {
            dir: tmp.path(),
            musttail_sink: Some(&sink),
            unit_cost_sink: None,
        },
    )?;

    let decisions = sink.into_inner().unwrap_or_default();
    Ok(MusttailRun {
        decisions,
        fn_types,
    })
}
