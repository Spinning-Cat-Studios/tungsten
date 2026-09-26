//! CLI type definitions for the Tungsten bootstrap compiler.
//!
//! Extracted from main.rs to keep the driver module focused on dispatch logic.

use clap::Subcommand;
use std::path::PathBuf;

mod expr;
mod options;

#[cfg(test)]
mod tests;

pub(crate) use expr::ExprCommands;
pub(crate) use options::{Cli, ColorMode};

use crate::explain;
use crate::info;
use tungsten_bootstrap::doctor;

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
    ///   tungsten test examples/list.tg --watchdog 300
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

        /// Force cost 3: `expect_type` only, runtime tests skipped. Rarely needed — `tg-test-tiers.toml` declares each file's tier (ADR 6.8.26c)
        #[arg(long)]
        check_only: bool,

        /// Fail (exit ≠ 0) when zero runnable tests are discovered — prevents
        /// a vacuous green run (ADR 2.7.26b T5b)
        #[arg(long)]
        require_tests: bool,

        /// Per-test wall-clock bound in seconds; a test that exceeds it is
        /// reported TIMEOUT instead of hanging. 0 disables (ADR 21.7.26f)
        #[arg(long, default_value_t = 60)]
        watchdog: u64,

        /// Print how many assertions each test actually EXECUTED, not just
        /// whether it passed (ADR 6.8.26b). A zero already fails the run on
        /// its own; the census exists for the non-zero rows — a test that
        /// executes 1 of its 3 assertions passes, gates green, and is two
        /// thirds imaginary
        #[arg(long)]
        assertion_census: bool,

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
        /// and print a sorted allocation report at program exit. The flag
        /// changes the emitted allocation shape: profiled, every site calls
        /// __tungsten_alloc unconditionally (the profiler records inside it);
        /// unprofiled, each site branches on the arena mode and calls malloc
        /// directly when it is off (ADR 18.9.26c). TUNGSTEN_ARENA=bump[:mib]
        /// at run time selects the bump arena (ADR 14.9.26b); the report ends
        /// with the arena's chunk/reserved/used/high-water tail.
        /// Optionally filter to a specific function: --alloc-profile=fn_name
        #[arg(long, value_name = "FN", num_args = 0..=1, default_missing_value = "", require_equals = true)]
        alloc_profile: Option<String>,

        /// Compile only the named codegen unit(s), skipping all others and the
        /// __mono depot. Requires --emit-llvm. Repeatable. Unit names as shown
        /// by `tungsten info codegen units` (e.g.
        /// parser__exprs__pratt__parse_unary_op). Cross-module declares and
        /// mono ownership still come from the full unit set, so the emitted IR
        /// matches a full build. For isolating one unit's memory/time during
        /// IR-construction profiling (ADR 3.7.26b).
        #[arg(long, value_name = "UNIT")]
        only_unit: Vec<String>,

        /// Print synthesized comparator Core terms as they are emitted (ADR
        /// 12.7.26c P6). These terms are built by the codegen-time comparator
        /// intercept AFTER elaboration, so `--dump-ir` cannot show them.
        /// Optionally filter to defs whose symbol contains a substring:
        /// --dump-synthesized=compare_List
        #[arg(long, value_name = "SYMBOL", num_args = 0..=1, default_missing_value = "", require_equals = true)]
        dump_synthesized: Option<String>,
    },

    /// Evaluate an expression or start an interactive REPL
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten run <file>` to evaluate a source file.\n\
                             The flat `tungsten eval <expr>` and `tungsten repl` spellings still work."
    )]
    Expr(ExprCommands),

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
    /// The flat spelling `tungsten expr eval` replaced (ADR 19.8.26a).
    #[command(name = "eval", hide = true)]
    EvalLegacy {
        /// The expression to evaluate
        expr: String,
    },

    /// The flat spelling `tungsten expr repl` replaced (ADR 19.8.26a).
    #[command(name = "repl", hide = true)]
    ReplLegacy,

    /// The flat spelling `tungsten cache clean-project` replaced (ADR
    /// 19.8.26a). Re-homed rather than sub-namespaced: it clears a build cache,
    /// so `cache` is where it always belonged, and sitting next to
    /// `cache clean` is what finally makes the scope difference visible instead
    /// of leaving it to two `after_help` paragraphs warning about each other.
    #[command(name = "clean", hide = true)]
    CleanLegacy {
        /// Entry source file whose project cache to clear (default: current directory)
        file: Option<PathBuf>,
    },

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
