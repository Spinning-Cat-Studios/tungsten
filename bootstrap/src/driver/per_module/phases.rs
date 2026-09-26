//! Stub Registration / Signature Collection helpers for per-module elaboration (ADR 5.5.26c).
//!
//! Signature Collection global collection (`run_signature_collection`), the verbose phase-result
//! logging (`log_stub_registration` / `log_signature_collection`), and the Signature-Collection-failure error
//! annotation (`annotate_errors_for_signature_collection_failure`) live here so
//! `mod.rs` keeps just the phase orchestration and the tree-walk scheduling
//! (ADR 29.6.26h repo-audit split).

use tungsten_core::Context;

use crate::driver::modules::ParsedModule;
use crate::driver::output::TraceOptions;
use crate::driver::pipeline::{self, BuildCtx};
use crate::elaborate::{ElabError, ModuleExports};

use super::accumulator::ModuleTreeAccumulator;

/// Log Stub Registration results (verbose + constructor trace).
pub(super) fn log_stub_registration(verbose: bool, trace: &TraceOptions, exports: &ModuleExports) {
    if verbose {
        eprintln!(
            "  Stub Registration: collected {} type stubs, {} constructor stubs",
            exports.types.len(),
            exports.constructors.len(),
        );
    }
    if trace.trace_ctor_registration {
        for (name, info) in &exports.constructors {
            eprintln!(
                "[ctor-reg] Stub Registration: register {} (parent={}, index={}) via collect_all_type_and_constructor_stubs",
                name, info.type_name, info.index
            );
        }
    }
}

/// Log Signature Collection results (verbose).
pub(super) fn log_signature_collection(verbose: bool, exports: &ModuleExports) {
    if verbose {
        eprintln!(
            "  Signature Collection: {} types, {} values, {} constructors after global collection",
            exports.types.len(),
            exports.values.len(),
            exports.constructors.len(),
        );
    }
}

/// Signature Collection: build combined AST from all modules and run global collection
/// to extract function signatures. Results are merged into `acc.exports`.
pub(super) fn run_signature_collection(
    module_tree: &ParsedModule,
    build: &BuildCtx<'_>,
    acc: &mut ModuleTreeAccumulator,
    verbose: bool,
) {
    let (combined_ast, combined_file_index) = pipeline::build_combined_ast(module_tree);
    let mut combined_module_info = build.module_info.clone();
    combined_module_info.item_index_to_file = combined_file_index;

    let mut ctx = Context::new();
    match crate::elaborate::collect_definitions_for_signature_collection(
        &combined_ast,
        &mut ctx,
        combined_module_info,
        &acc.exports,
    ) {
        Ok(mut collected) => {
            // The collection pass defers its errors rather than returning them
            // (ADR 14.8.26g D2), so `Ok` is not evidence of a clean pass —
            // drain and report them here, exactly once (D2a).
            let errors = collected.take_collection_errors();
            if !errors.is_empty() {
                warn_signature_collection_failed(&errors, verbose);
                acc.signature_collection_ok = false;
                // D4: the failure is an error, not only a warning — the
                // dropped errors enter the run's list at their own spans,
                // ahead of the per-module groups.
                acc.pre_walk_errors.extend(errors);
            }
            // Merge what DID collect either way: partial signatures resolve
            // more downstream references than none, and the accumulating walk
            // (D1) reports the remainder at their real spans.
            let global_exports = collected.extract_value_exports();
            if verbose {
                eprintln!(
                    "  Signature Collection: global collection {}, {} types, {} values, {} constructors",
                    if acc.signature_collection_ok { "succeeded" } else { "partially collected" },
                    global_exports.types.len(),
                    global_exports.values.len(),
                    global_exports.constructors.len(),
                );
            }
            acc.merge_exports(global_exports);
        }
        Err(errors) => {
            // The conditional deferral's short-circuit arm (ADR 14.8.26g D2
            // as tightened at P3): an unpoisoned error set. No exports merge
            // on this arm.
            warn_signature_collection_failed(&errors, verbose);
            acc.signature_collection_ok = false;
            // D4: reported as errors here too.
            acc.pre_walk_errors.extend(errors);
        }
    }
}

/// Warn that Signature Collection failed (ADR 13.5.26g §2.1).
fn warn_signature_collection_failed(errors: &[ElabError], verbose: bool) {
    eprintln!("{}", signature_collection_warning(errors, verbose));
}

/// The Signature Collection failure warning, as text — built apart from the
/// `eprintln!` so the count, pluralisation and verbose expansion are
/// assertable rather than write-only stderr.
pub(super) fn signature_collection_warning(errors: &[ElabError], verbose: bool) -> String {
    let count = errors.len();
    let mut warning = format!(
        "warning: Signature Collection global collection failed with {} error{}; \
         cross-module imports may not resolve.",
        count,
        if count == 1 { "" } else { "s" },
    );
    if let Some(first) = errors.first() {
        warning.push_str(&format!("\n  first error: {}", first));
    }
    warning.push_str(
        "\n  hint: run `tungsten doctor check module signature-collection <file>` for details",
    );
    if verbose {
        for e in errors {
            warning.push_str(&format!("\n    - {}", e));
        }
    }
    warning
}

/// Annotate "not found" errors with a Signature Collection failure hint (ADR 13.5.26g §2.2).
///
/// When Signature Collection global collection fails, downstream modules can't resolve
/// cross-module imports, producing misleading E0001/E0005/E0006 errors.
/// This adds a note to those errors pointing to the real root cause.
pub(super) fn annotate_errors_for_signature_collection_failure(errors: &mut [ElabError]) {
    use crate::elaborate::ElabErrorKind;
    use crate::elaborate::Note;

    let hint = "Signature Collection global collection failed — this error may be caused by \
                a bad import in another module. Run `tungsten doctor check module signature-collection <file>` \
                for details.";

    for err in errors.iter_mut() {
        let is_resolution_error = matches!(
            &err.kind,
            ElabErrorKind::UndefinedVariable(_)
                | ElabErrorKind::ModuleNotFound { .. }
                | ElabErrorKind::ItemNotFoundInModule { .. }
                | ElabErrorKind::UnresolvedImport(_)
        );
        if is_resolution_error {
            err.notes.push(Note {
                message: hint.to_string(),
                span: None,
                file_path: None,
            });
        }
    }
}

/// Annotate the Body-Elaboration errors with a Signature-Collection-failure hint
/// **only when Signature Collection actually failed** (`ok == false`). Extracted
/// from the driver's `map_err` so the *gating* — not just the annotation — is
/// directly unit-testable (ADR 13.5.26g §2.2).
pub(super) fn annotate_if_signature_collection_failed(ok: bool, errors: &mut [ElabError]) {
    if !ok {
        annotate_errors_for_signature_collection_failure(errors);
    }
}

/// What to tell the reader about the part of the project the walk never saw.
///
/// Since ADR 14.8.26g the walk accumulates and reaches every module, so this
/// note should never render — it stays as a tripwire (and ADR 14.8.26g's own
/// acceptance test): if a future change reintroduces an early exit, a failing
/// run says so in ordinary output instead of silently reporting on a fraction
/// of the tree (the pre-14.8.26g behaviour, measured in ADR 7.8.26d's
/// retrospective). Returns `None` when the walk reached everything — a
/// complete run must read exactly as it did before.
pub(super) fn unreached_note(total: usize, walked: usize) -> Option<String> {
    let unreached = total.checked_sub(walked).filter(|n| *n > 0)?;
    // The noun agrees with the total, the verb with the unreached count:
    // "1 of 3 modules was", "249 of 263 modules were", "1 of 1 module was".
    Some(format!(
        "note: {unreached} of {total} module{} {} not elaborated — \
         this run did not examine the whole project",
        if total == 1 { "" } else { "s" },
        if unreached == 1 { "was" } else { "were" },
    ))
}

/// Finish a failed Body Elaboration: annotate what is misleading, and say what
/// was never looked at. One function so the driver's `map_err` stays a call.
///
/// Returns the note it printed, or `None` when the walk was complete. The
/// caller discards it — the return exists so a test can assert *which* of the
/// two effects happened; a function whose whole body is effects is one nothing
/// can distinguish from an empty one.
pub(super) fn finish_failed_body_elaboration(
    acc: &ModuleTreeAccumulator,
    module_tree: &ParsedModule,
    errors: &mut [ElabError],
) -> Option<String> {
    annotate_if_signature_collection_failed(acc.signature_collection_ok, errors);
    let note = unreached_note(module_tree.module_count(), acc.modules_walked)?;
    eprintln!("{note}");
    Some(note)
}
