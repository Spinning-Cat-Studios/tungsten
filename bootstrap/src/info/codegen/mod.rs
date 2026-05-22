//! Codegen-gated info subcommands (ADR 12.5.26h).
//!
//! Groups `info codegen units`, `info codegen mono`, `info codegen abi`,
//! and `info codegen symbols` into one `#[cfg(feature = "codegen")]` module.

pub(crate) mod mono;
pub(crate) mod units;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};

/// Shared arguments for `info codegen abi` / `info abi` (hidden legacy alias).
#[derive(Args)]
pub struct AbiArgs {
    /// Function name to analyze (omit for --all)
    pub function_name: Option<String>,

    /// The LLVM IR (.ll) file to analyze
    pub file: PathBuf,

    /// Analyze all functions in the file
    #[arg(long)]
    pub all: bool,

    /// Invoke llc for register assignment details (Tier 2)
    #[arg(long)]
    pub deep: bool,
}

/// Codegen-related info subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten info codegen <subcommand>`. All require
/// the `codegen` feature.
#[derive(Subcommand)]
pub enum InfoCodegenCommands {
    /// Show lambda → source name mapping
    ///
    /// Prints a table mapping IR function names (__lambda_N) to their
    /// source-level names and locations.
    ///
    /// Examples:
    ///   tungsten info codegen symbols examples/hello.tg
    Symbols {
        /// The source file to inspect
        file: PathBuf,
    },

    /// Inspect ABI layout and passing decisions for functions in an LLVM IR file
    ///
    /// Shows struct layouts, ABI passing decisions (DIRECT vs INDIRECT),
    /// and optionally register assignments via llc.
    ///
    /// Examples:
    ///   tungsten info codegen abi main hello.ll
    ///   tungsten info codegen abi hello.ll --all
    Abi(AbiArgs),

    /// Show codegen unit partitioning (ADR 6.5.26d §2.4)
    ///
    /// Displays per-module codegen unit names, definition counts, and
    /// stable-sorted definition names. Requires multi-module elaboration.
    ///
    /// Examples:
    ///   tungsten info codegen units src/compiler/main.tg
    Units {
        /// The root source file of the project
        file: PathBuf,
    },

    /// Display mono request table and ownership map (ADR 8.5.26i)
    ///
    /// Shows all monomorphized instances, their owner units, mangled
    /// symbols, and type arguments.
    ///
    /// Examples:
    ///   tungsten info codegen mono src/compiler/main.tg
    Mono {
        /// The root source file of the project
        file: PathBuf,
    },
}

/// Dispatch a codegen-related info subcommand.
pub fn dispatch_codegen_info(
    cmd: InfoCodegenCommands,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    match cmd {
        InfoCodegenCommands::Symbols { file } => {
            super::commands::cmd_info_symbols(&file, verbose, max_errors)
        }
        InfoCodegenCommands::Abi(args) => crate::dump_abi::cmd_dump_abi(
            args.function_name.as_deref(),
            &args.file,
            args.all,
            args.deep,
        ),
        InfoCodegenCommands::Units { file } => {
            units::cmd_info_codegen_units(&file, verbose, max_errors)
        }
        InfoCodegenCommands::Mono { file } => mono::cmd_info_mono(&file, verbose, max_errors),
    }
}
