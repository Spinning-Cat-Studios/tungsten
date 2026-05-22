//! CLI type definitions for the Tungsten bootstrap compiler.
//!
//! Extracted from main.rs to keep the driver module focused on dispatch logic.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::explain;
use crate::info;
use tungsten_bootstrap::doctor;

/// Controls when ANSI color codes are emitted in test output.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ColorMode {
    /// Color when stdout is a TTY (default)
    Auto,
    /// Always emit color codes
    Always,
    /// Never emit color codes
    Never,
}

#[derive(Parser)]
#[command(name = "tungsten")]
#[command(author, version, about = "The Tungsten proof language compiler")]
#[command(
    long_about = "Tungsten is a proof language that combines programming and theorem proving.\n\n\
                  This is the bootstrap compiler, written in Rust. Once Tungsten is self-hosting,\n\
                  it will be replaced by a compiler written in Tungsten itself."
)]
#[command(
    after_help = "Core commands: check, run, test, compile, eval, repl, clean, cache\n\
                  Diagnostics:   info, explain, doctor, diff, commands\n\
                  Experience:    sidecar\n\n\
                  Run `tungsten <command> --help` for details on a specific command.\n\
                  Run `tungsten commands` for a flat listing of all commands.\n\
                  Run `tungsten info pipeline` for diagnostic flags and inspection tools."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run a file directly (shorthand for `tungsten run <FILE>`)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Show verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Maximum number of errors to display (0 = no limit)
    #[arg(long, global = true, default_value = "20")]
    pub max_errors: usize,

    /// Dump elaborated type annotations to stderr (diagnostic)
    #[arg(long, global = true)]
    pub dump_types: bool,

    /// Always show diagnostic hints in error output (even in non-TTY contexts)
    #[arg(long, global = true)]
    pub hints: bool,

    /// Suppress diagnostic hints in error output
    #[arg(long, global = true)]
    pub no_hints: bool,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Type-check a file without running
    Check {
        /// The source file to check
        file: PathBuf,

        /// Disable build cache (force full recompilation)
        #[arg(long)]
        no_cache: bool,

        /// Output errors as JSON with structured diagnostic hints
        #[arg(long)]
        json: bool,
    },

    /// Type-check and evaluate a file
    Run {
        /// The source file to run
        file: PathBuf,

        /// Disable build cache (force full recompilation)
        #[arg(long)]
        no_cache: bool,
    },

    /// Discover and run test_* functions in a file
    ///
    /// Discovers top-level test_* functions (arity 0, returns Unit),
    /// elaborates with `ElabMode::Test` to evaluate `expect_type` assertions,
    /// and reports pass/fail for each test.
    ///
    /// Examples:
    ///   tungsten test examples/list.tg
    ///   tungsten test examples/list.tg --filter inference
    ///   tungsten test examples/list.tg --check-only
    Test {
        /// The source file containing tests
        file: PathBuf,

        /// Filter tests by name substring
        #[arg(long)]
        filter: Option<String>,

        /// Scope test discovery to a specific module source file
        /// (e.g. "src/compiler/elab/env/mod.tg")
        #[arg(long)]
        module: Option<String>,

        /// Only run `expect_type` checks (no codegen); runtime tests are skipped
        #[arg(long)]
        check_only: bool,

        /// When to use color in test output
        #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
        color: ColorMode,
    },

    /// Compile a file to a native executable
    #[cfg(feature = "codegen")]
    Compile {
        /// The source file to compile
        file: PathBuf,

        /// Output file path (defaults to input name without extension)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit LLVM IR instead of executable
        #[arg(long)]
        emit_llvm: bool,

        /// Check for TyVar escapes after elaboration (always-on in debug builds)
        #[arg(long)]
        check_tyvar_escape: bool,

        /// Show codegen backtraces on TyVar fallthrough warnings
        #[arg(long)]
        codegen_backtrace: bool,

        /// Pretty-print Core IR for a named definition (comma-separated)
        #[arg(long, value_name = "NAME")]
        dump_ir: Option<String>,

        /// Trace type transformations during elaboration for a named definition
        #[arg(long, value_name = "NAME")]
        trace_types: Option<String>,

        /// Show encoding breakdown for a named ADT (ADR 13.4.26c §4a)
        #[arg(long, value_name = "NAME")]
        dump_encoding: Option<String>,

        /// Emit DWARF line tables for source-level debugging (T1)
        #[arg(long)]
        debug_info: bool,

        /// Enable AddressSanitizer instrumentation (T2)
        #[arg(long)]
        sanitize: bool,

        /// Trace ADT construct/match operations at runtime (T3).
        /// Optionally filter by ADT name (e.g., --trace-adt-ops=Item).
        #[arg(long, value_name = "TYPE", num_args = 0..=1, default_missing_value = "all")]
        trace_adt_ops: Option<String>,

        /// Trace type encoding decisions during elaboration (ADR 18.4.26h §3).
        /// Shows encoding stack, cycle detection, and μ-variable assignments.
        /// Optionally filter by type name (e.g., --trace-encoding=TypeExpr).
        #[arg(long, value_name = "TYPE", num_args = 0..=1, default_missing_value = "")]
        trace_encoding: Option<String>,

        /// Trace normalization path for a specific type (ADR 20.4.26c).
        /// Shows step-by-step normalization decisions including cycle detection,
        /// cache lookups, and type expansions.
        /// Optionally filter by type name (e.g., --trace-normalization=TypeExpr).
        #[arg(long, value_name = "TYPE", num_args = 0..=1, default_missing_value = "")]
        trace_normalization: Option<String>,

        /// Trace constructor registration during elaboration (ADR 7.5.26e).
        /// Shows which phase registers each constructor and via which code path.
        #[arg(long)]
        trace_constructor_registration: bool,

        /// Trace musttail TCO decisions during codegen (ADR 8.5.26c).
        /// Reports which self-recursive calls get musttail and why others are skipped.
        #[arg(long)]
        trace_musttail: bool,

        /// Trace escape analysis decisions during codegen (ADR 8.5.26d).
        /// Reports which fold allocations use stack vs heap.
        #[arg(long)]
        trace_escape: bool,

        /// Trace monomorphization pipeline decisions during codegen (ADR 8.5.26g).
        /// Reports discovery, ownership assignment, and symbol generation.
        #[arg(long)]
        trace_mono: bool,

        /// Use source-level names for lambda functions in LLVM IR.
        /// Makes backtraces and IR dumps more readable.
        #[arg(long)]
        named_lambdas: bool,

        /// Stop after Core IR generation; skip LLVM codegen and linking.
        /// All elaboration and encoding diagnostics are still available.
        /// Incompatible with --emit-llvm.
        #[arg(long)]
        no_codegen: bool,

        /// Enable allocation profiling: emit per-function allocation hooks
        /// and print a sorted allocation report at program exit.
        /// Optionally filter to a specific function: --alloc-profile=fn_name
        #[arg(long, value_name = "FN", num_args = 0..=1, default_missing_value = "", require_equals = true)]
        alloc_profile: Option<String>,
    },

    /// Evaluate an expression
    Eval {
        /// The expression to evaluate
        expr: String,
    },

    /// Start interactive REPL
    Repl,

    /// Clear the build cache
    Clean,

    /// Manage the build cache
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Query information about types, definitions, and encodings
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten doctor` for health checks, `tungsten explain` for documentation.\n\
                             Run `tungsten info pipeline` for the full diagnostic reference."
    )]
    Info(info::InfoCommands),

    /// Explain errors and type representations step by step
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten info` for type inspection, `tungsten doctor` for health checks."
    )]
    Explain(explain::ExplainCommands),

    /// Run compiler diagnostics and health checks
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten info` for read-only inspection, `tungsten explain` for documentation."
    )]
    Doctor(doctor::DoctorCommands),

    /// Structural comparison of compiler-produced artifacts
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten info` for type inspection, `tungsten doctor` for health checks."
    )]
    Diff(DiffCommands),

    /// Manage the agent experience store (session recording, stats)
    #[command(subcommand)]
    #[command(after_help = "See also: `tungsten doctor suggest-tools` for tool recommendations.")]
    Sidecar(tungsten_bootstrap::sidecar::SidecarCommands),

    /// List all available commands
    #[command(name = "commands")]
    ListCommands {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show as tree hierarchy
        #[arg(long)]
        tree: bool,
    },

    // --- Hidden aliases for backward compatibility ---
    /// Compare two LLVM IR files structurally (type defs + function signatures)
    #[command(hide = true)]
    DiffIr {
        /// Baseline IR file
        file_a: PathBuf,

        /// Candidate IR file
        file_b: PathBuf,

        /// Only compare type definitions
        #[arg(long)]
        types_only: bool,

        /// Only compare function signatures
        #[arg(long)]
        signatures_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compare two Core IR dump files structurally (from --dump-ir output)
    #[command(hide = true)]
    DiffCore {
        /// Baseline Core IR dump file
        file_a: PathBuf,

        /// Candidate Core IR dump file
        file_b: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect ABI layout and passing decisions for functions in an LLVM IR file
    #[cfg(feature = "codegen")]
    #[command(hide = true)]
    DumpAbi {
        /// Function name to analyze (omit for --all)
        function_name: Option<String>,

        /// The LLVM IR (.ll) file to analyze
        file: PathBuf,

        /// Analyze all functions in the file
        #[arg(long)]
        all: bool,

        /// Invoke llc for register assignment details (Tier 2)
        #[arg(long)]
        deep: bool,
    },
}

mod subcommands;
pub(crate) use subcommands::CacheCommands;
pub(crate) use subcommands::DiffCommands;
