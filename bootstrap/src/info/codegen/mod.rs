//! Codegen-gated info subcommands (ADR 12.5.26h).
//!
//! Groups `info codegen units`, `info codegen mono`, `info codegen abi`,
//! and `info codegen symbols` into one `#[cfg(feature = "codegen")]` module.

pub(crate) mod mono;
pub(crate) mod symbols;
pub(crate) mod unit_paths;
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
    /// Show lambda → source name mapping, or one function's full symbol set
    ///
    /// Default: a table mapping IR function names (__lambda_N) to their
    /// source-level names and locations.
    ///
    /// With --by-function NAME: every LLVM symbol that ONE source function
    /// compiles to — the closure-returning wrapper, `$direct`, `$direct_mt`,
    /// and each of its lambdas — with the role of each. This is the set a
    /// `perf` profile's self time must be summed across to attribute cost to a
    /// source function; summing only `<name>` and `<name>$direct` under-counts,
    /// and `$direct_mt` is usually where a hot self-recursive loop's samples
    /// actually land (ADR 5.8.26b measured 56.94% there).
    ///
    /// Examples:
    ///   tungsten info codegen symbols examples/hello.tg
    ///   tungsten info codegen symbols src/compiler/main.tg --by-function import_list_lookup
    Symbols {
        /// The source file to inspect
        file: PathBuf,

        /// Report the complete symbol set for this one source function.
        #[arg(long, value_name = "NAME")]
        by_function: Option<String>,
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

    /// Show where each codegen unit's `.ll` lands, and which units collide
    ///
    /// `--emit-llvm` writes one `.ll` per codegen unit into a mirror of the
    /// source tree. Two units whose paths differ only in case overwrite each
    /// other on a case-insensitive filesystem (APFS, NTFS) — the self-hosted
    /// compiler emits 2,055 units into 2,049 files for exactly that reason, and
    /// a unit that loses a collision is one NO `doctor check ir` audit ever
    /// sees. This reports every destination and groups the contenders, using the
    /// same derivation the emitter uses so it cannot drift.
    ///
    /// Exits non-zero on any collision or unplaceable unit, so CI can gate on
    /// it. Cost 3 — elaborate only, no LLVM.
    ///
    /// See also: `tungsten info codegen units` (the partitioning itself),
    /// `make check-ir-audits` (the audits that walk the emitted tree).
    ///
    /// Examples:
    ///   tungsten info codegen unit-paths src/compiler/main.tg
    ///   tungsten info codegen unit-paths src/compiler/main.tg -o target/ll-audit --json
    #[command(name = "unit-paths")]
    UnitPaths {
        /// The root source file of the project
        file: PathBuf,

        /// Output directory the paths are planned against (default: the same
        /// `target/ll` that `--emit-llvm` would use)
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,

        /// Emit JSON
        #[arg(long)]
        json: bool,
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

    /// Drill into one function's musttail eligibility (ADR 1.7.26b §2.3)
    ///
    /// Compiles with codegen and reports whether the function's lowered
    /// signature passes `check_musttail_abi_safety`; if not, names each
    /// return/param blocker and whether it is flattenable (decomposable).
    /// One block per monomorph specialization when the name resolves to more
    /// than one lowered signature.
    ///
    /// See also: `tungsten doctor check tco-coverage` (whole-file ranking).
    ///
    /// Examples:
    ///   tungsten info codegen musttail-eligibility collect_type_names src/compiler/main.tg
    #[command(name = "musttail-eligibility")]
    MusttailEligibility {
        /// The source-level function name to drill into.
        function: String,

        /// The root source file of the project (must contain a `main`).
        file: PathBuf,
    },

    /// Show a function's Class-P indirect-parameter lowering (ADR 1.7.26e).
    ///
    /// Compiles with codegen and reports, per source parameter, whether it is
    /// passed by-value, decomposed into scalar fields (18.5.26a), or indirect via
    /// a caller-owned buffer `ptr` (1.7.26e), plus the `$direct_mt` slot layout
    /// (`[sret]? indirect… env after-env…`). One block per monomorph specialization.
    ///
    /// See also: `tungsten info codegen musttail-eligibility` (lowered signature +
    /// blockers), `tungsten doctor check tco-coverage` (whole-file ranking).
    ///
    /// Examples:
    ///   tungsten info codegen indirect-abi collect_type_names src/compiler/main.tg
    #[command(name = "indirect-abi")]
    IndirectAbi {
        /// The source-level function name to inspect.
        function: String,

        /// The root source file of the project (must contain a `main`).
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
        InfoCodegenCommands::Symbols { file, by_function } => {
            super::commands::cmd_info_symbols(&file, by_function.as_deref(), verbose, max_errors)
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
        InfoCodegenCommands::UnitPaths { file, output, json } => {
            unit_paths::cmd_info_codegen_unit_paths(
                &file,
                output.as_deref(),
                json,
                verbose,
                max_errors,
            )
        }
        InfoCodegenCommands::Mono { file } => mono::cmd_info_mono(&file, verbose, max_errors),
        InfoCodegenCommands::MusttailEligibility { function, file } => {
            crate::compile::tco::cmd_musttail_eligibility(&function, &file, verbose, max_errors)
        }
        InfoCodegenCommands::IndirectAbi { function, file } => {
            crate::compile::tco::cmd_indirect_abi(&function, &file, verbose, max_errors)
        }
    }
}
