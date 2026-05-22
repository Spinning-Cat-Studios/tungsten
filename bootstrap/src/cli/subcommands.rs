//! Cache and Diff CLI subcommand definitions.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum CacheCommands {
    /// Show cache statistics (AST + elaboration)
    Stats {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show cache status summary (alias for stats, ADR 10.5.26l)
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Prune cache to target size (removes least recently used entries)
    Prune {
        /// Target size in MB (defaults to configured `max_size_mb`)
        #[arg(long)]
        target_mb: Option<u64>,
    },

    /// Recursively find and remove all .tungsten cache directories (skips target/)
    Clean {
        /// Show what would be removed without actually deleting
        #[arg(long)]
        dry_run: bool,
    },
}

use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum DiffCommands {
    /// Compare two LLVM IR files structurally (type defs + function signatures)
    Ir {
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
    Core {
        /// Baseline Core IR dump file
        file_a: PathBuf,

        /// Candidate Core IR dump file
        file_b: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Structural tree-diff of two type encodings
    ///
    /// Compares the cached encodings of two named types and shows
    /// an inline diff with +/- markers at divergence points.
    /// Returns exit code 0 if identical, nonzero if different.
    ///
    /// Examples:
    ///   tungsten diff types `TypeA` `TypeB` examples/list.tg
    ///   tungsten diff types Expr `TypeExpr` src/compiler/main.tg
    Types {
        /// First type name
        type_a: String,

        /// Second type name
        type_b: String,

        /// The source file containing both types
        file: PathBuf,
    },

    /// Compare ABI layout between bootstrap codegen and .tg emitter (ADR 13.5.26k)
    ///
    /// Elaborates the file, computes the bootstrap codegen layout for the
    /// named type, and compares it with the self-hosted emitter's ABI manifest.
    ///
    /// Examples:
    ///   tungsten diff abi Nat examples/hello.tg
    ///   tungsten diff abi Option src/compiler/main.tg
    #[cfg(feature = "codegen")]
    #[command(
        after_help = "See also: `tungsten info type adt` for ADT details, `tungsten info codegen abi` for IR-level ABI."
    )]
    Abi {
        /// Type name to compare (e.g., "Nat", "Option", "List")
        type_name: String,

        /// The source file to elaborate
        file: PathBuf,
    },

    /// Compare L1 and tungsten1 elaboration results on a file (ADR 20.5.26a)
    ///
    /// Runs L1 (this binary) and tungsten1 (self-compiled binary) on the same
    /// file in check mode, then compares error counts and messages. Helps
    /// identify codegen regressions in the self-hosted compiler.
    ///
    /// Cost 3+3 (two elaboration passes).
    ///
    /// Examples:
    ///   tungsten diff l1-l2-check src/compiler/main.tg --l2-binary ./tungsten1
    ///   tungsten diff l1-l2-check examples/hello.tg --l2-binary ./tungsten1
    #[command(name = "l1-l2-check")]
    L1L2Check {
        /// The source file to check with both compilers
        file: PathBuf,

        /// Path to the tungsten1 (self-compiled) binary
        #[arg(long, default_value = "./tungsten1")]
        l2_binary: PathBuf,
    },
}
