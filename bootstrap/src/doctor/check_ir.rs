//! `tungsten doctor check ir` — IR-related health checks (ADR 12.5.26h).
//!
//! Groups IR validation checks under `doctor check ir ...`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use super::checks;

/// IR-related health check subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten doctor check ir <subcommand>`.
#[derive(Subcommand)]
pub enum CheckIrCommands {
    /// Check store/load type-width consistency in LLVM IR (ADR 21.4.26b)
    ///
    /// Parses an emitted .ll file and reports store/load instructions
    /// where the value type width disagrees with the pointer target type.
    ///
    /// Examples:
    ///   tungsten doctor check ir layout output.ll
    Layout {
        /// The LLVM IR (.ll) file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Validate per-unit declaration hygiene in emitted LLVM IR (ADR 10.5.26b)
    ///
    /// Scans `.ll` files in a directory and reports any direct `call @symbol`
    /// target that lacks a matching `declare` or `define` in the same file.
    ///
    /// Examples:
    ///   tungsten doctor check ir declares --from-existing-ir target/ll/
    ///
    /// See also: `tungsten compile --emit-llvm`, `tungsten info codegen units`
    Declares {
        /// Directory containing `.ll` files to scan
        #[arg(long)]
        from_existing_ir: PathBuf,
    },
    /// Scan for null function pointer calls in emitted LLVM IR (ADR 10.5.26d)
    ///
    /// Searches `.ll` files for `call.*null(` patterns that indicate
    /// unresolved monomorphization — a mono instance was expected but
    /// the function pointer was never filled in.
    ///
    /// Examples:
    ///   tungsten doctor check ir null-calls target/ll/
    NullCalls {
        /// Directory containing `.ll` files to scan
        dir: PathBuf,
    },
}

/// Dispatch an IR-related health check subcommand.
pub fn dispatch_check_ir(cmd: CheckIrCommands) -> ExitCode {
    match cmd {
        CheckIrCommands::Layout { file, json } => {
            checks::check_ir_layout::cmd_check_ir_layout(&file, json)
        }
        CheckIrCommands::Declares { from_existing_ir } => {
            checks::check_declares::cmd_check_declares(&from_existing_ir)
        }
        CheckIrCommands::NullCalls { dir } => checks::check_null_calls::cmd_check_null_calls(&dir),
    }
}
