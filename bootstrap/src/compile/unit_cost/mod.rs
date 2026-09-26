//! `doctor check unit-cost` — ranked per-unit codegen cost census
//! (ADR 8.7.26a §2.1).
//!
//! Productizes the ADR 3.7.26b hand-rolled census (verbose stderr → awk →
//! manual transcription): runs stage-1 codegen at `jobs = 1` collecting
//! per-unit wall time + allocation volume, then reports a ranked table,
//! JSON, or the `TUNGSTEN_CODEGEN_SERIAL_UNITS` value. The pure report
//! logic lives in `tungsten_core::diagnostics::unit_cost` (gate-scoped);
//! this module is the codegen-side glue, mirroring `compile/tco/`.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::diagnostics::unit_cost::{
    gate_fails, render_json, render_serial_list, render_table, sort_ranked, CostThreshold,
};

mod collect;

/// The `--threshold` default for `--emit-serial-list` when none is given —
/// the ADR 3.7.26b census convention (57 units ≥ 0.5 s).
const SERIAL_LIST_DEFAULT_THRESHOLD: CostThreshold = CostThreshold::WallTimeSecs(0.5);

/// CLI options for `doctor check unit-cost`.
#[derive(Debug, Default, Clone)]
pub(crate) struct UnitCostOpts {
    /// Emit machine-readable JSON instead of the table.
    pub(crate) json: bool,
    /// Threshold gate: `0.5s` (wall time) or `8GB` (allocation volume).
    /// Filters the table to units meeting it and exits non-zero when any do.
    pub(crate) threshold: Option<String>,
    /// Print the comma-separated `TUNGSTEN_CODEGEN_SERIAL_UNITS` value for
    /// units meeting the threshold (default 0.5s) instead of the table.
    pub(crate) emit_serial_list: bool,
}

/// Entry point for `tungsten doctor check unit-cost <file>`.
pub(crate) fn cmd_check_unit_cost(
    file: &PathBuf,
    opts: &UnitCostOpts,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let threshold = match opts.threshold.as_deref().map(CostThreshold::parse) {
        Some(Ok(t)) => Some(t),
        Some(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        None => None,
    };

    let mut records = match collect::collect_unit_costs(file, verbose, max_errors) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    sort_ranked(&mut records, threshold.as_ref());

    if opts.emit_serial_list {
        // A generator, not a gate: always exits 0 so it composes in scripts.
        let list_threshold = threshold.unwrap_or(SERIAL_LIST_DEFAULT_THRESHOLD);
        println!("{}", render_serial_list(&records, &list_threshold));
        return ExitCode::SUCCESS;
    }

    if opts.json {
        println!("{}", render_json(&records));
    } else {
        print!("{}", render_table(&records, threshold.as_ref()));
    }

    // Threshold doubles as the CI gate (same shape as tco-coverage --gate):
    // exit non-zero when any unit meets it.
    match threshold {
        Some(t) if gate_fails(&records, &t) => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    }
}
