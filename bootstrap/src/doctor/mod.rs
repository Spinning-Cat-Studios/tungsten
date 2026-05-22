//! `tungsten doctor` — diagnostic namespace for compiler health checks.
//!
//! Sub-namespaces under `doctor check` (ADR 12.5.26h):
//! - `doctor check type ...` — type-system health checks
//! - `doctor check ir ...` — IR validation checks
//!
//! Legacy flat paths (e.g., `doctor check stubs`) remain as hidden aliases.
//! See ADR 16.4.26b for original design rationale.

pub mod audit_mutual_types;
mod audit_recursion;
mod check_ir;
mod check_type;
pub mod checks;
pub mod diff_types;
mod dispatch;
mod map_span;
mod module_overlap;
mod self_test;
pub(crate) mod suggest_tools;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

pub use check_ir::CheckIrCommands;
pub use check_type::{CheckTypeCommands, EncodingDepthArgs};
pub use dispatch::cmd_doctor;

#[derive(Subcommand)]
pub enum DoctorCommands {
    /// Run self-test suite on example programs
    ///
    /// Exercises the compiler on known-good programs, running each through
    /// parse → check → compile → run and verifying expected output.
    ///
    /// Default tier runs 3 core programs. Use --full for all registered programs.
    ///
    /// Examples:
    ///   tungsten doctor self-test
    ///   tungsten doctor self-test --full
    ///   tungsten doctor self-test --json
    SelfTest {
        /// Run full tier (all registered programs, not just default 3)
        #[arg(long)]
        full: bool,

        /// Verbose output (show command outputs)
        #[arg(short, long)]
        verbose: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Identify and classify all recursive functions
    ///
    /// Builds a call graph, finds recursive groups, and classifies each:
    /// - TAIL-RECURSIVE: musttail eligible, constant stack
    /// - TREE-RECURSIVE: O(tree height) stack depth
    /// - LINEAR NON-TAIL: O(n) stack depth
    /// - GENERAL: needs manual analysis
    ///
    /// Examples:
    ///   tungsten doctor audit-recursion examples/list.tg
    ///   tungsten doctor audit-recursion src/compiler/main.tg
    AuditRecursion {
        /// The source file to analyze
        file: PathBuf,
    },

    /// Identify mutually recursive type groups
    ///
    /// Builds a type dependency graph from ADT constructor fields,
    /// finds strongly connected components, and reports:
    /// - Mutually recursive clusters (>1 type)
    /// - Self-recursive types (single type referencing itself)
    /// - Non-recursive leaf types
    ///
    /// Examples:
    ///   tungsten doctor audit-mutual-types examples/list.tg
    ///   tungsten doctor audit-mutual-types src/compiler/main.tg
    AuditMutualTypes {
        /// The source file to analyze
        file: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Structural tree-diff of two type encodings (use `tungsten diff types` instead)
    #[command(hide = true)]
    DiffTypes {
        /// First type name
        type_a: String,

        /// Second type name
        type_b: String,

        /// The source file containing both types
        file: PathBuf,
    },

    /// Suggest diagnostic tools for an error description (ADR 21.4.26d)
    ///
    /// Maps a free-text error description to ranked diagnostic commands.
    /// Uses keyword matching against a static pattern registry. Cost 1
    /// (no file I/O, no elaboration). Designed for AI agent consumption.
    ///
    /// Examples:
    ///   tungsten doctor suggest-tools "SIGSEGV when running compiled program"
    ///   tungsten doctor suggest-tools "type mismatch" --json
    ///   tungsten doctor suggest-tools "stack overflow in recursive function"
    SuggestTools {
        /// Free-text error description to match against
        description: String,

        /// Output results as JSON (for agent consumption)
        #[arg(long)]
        json: bool,
    },

    /// Map a byte offset to file:line:col (ADR 4.5.26b retrospective)
    ///
    /// Converts a span byte offset to a human-readable file:line:col location.
    /// Use --project to search all module files in a project tree.
    ///
    /// Examples:
    ///   tungsten doctor map-span src/compiler/elab/exprs/tuples.tg 1234
    ///   tungsten doctor map-span src/compiler/main.tg 5678 --project
    MapSpan {
        /// The source file (or project main file with --project)
        file: PathBuf,

        /// Byte offset to look up
        offset: u32,

        /// Search all module files in the project tree
        #[arg(long)]
        project: bool,
    },

    /// Run compiler health checks (sub-namespace for all check-* commands)
    ///
    /// Validates compiler invariants, encoding consistency, phase correctness,
    /// and IR hygiene. Each check targets a specific subsystem.
    ///
    /// Examples:
    ///   tungsten doctor check stubs examples/hello.tg
    ///   tungsten doctor check fold-consistency examples/list.tg
    ///   tungsten doctor check declares --from-existing-ir target/ll/
    ///
    /// See also: `tungsten info` for read-only inspection, `tungsten explain` for documentation.
    #[command(subcommand)]
    Check(CheckCommands),
}

/// Health check subcommands grouped under `tungsten doctor check`.
///
/// Sub-namespaces (ADR 12.5.26h):
/// - `doctor check type ...` — type-system checks
/// - `doctor check ir ...` — IR validation checks
#[derive(Subcommand)]
pub enum CheckCommands {
    // ── Visible grouped sub-namespaces ──
    /// Type-system health checks
    ///
    /// Sub-namespace for type encoding, normalization, phase invariant,
    /// and constructor validation checks.
    /// See `tungsten doctor check type --help` for details.
    #[command(subcommand)]
    Type(CheckTypeCommands),

    /// IR validation checks
    ///
    /// Sub-namespace for LLVM IR layout and declaration hygiene checks.
    /// See `tungsten doctor check ir --help` for details.
    #[command(subcommand)]
    Ir(CheckIrCommands),

    // ── Visible top-level checks ──
    /// Check pub use re-export completeness (ADR 8.5.26f)
    ///
    /// Walks the module tree and checks that every `pub use` declaration
    /// actually copied items. Reports declarations that resolved to zero
    /// items or had missing named imports.
    ///
    /// Examples:
    ///   tungsten doctor check reexport-completeness src/compiler/main.tg
    ReexportCompleteness {
        /// The root source file to check
        file: PathBuf,
    },

    /// Check for symbol collisions across object files (ADR 6.5.26d §2.5)
    ///
    /// Runs `nm -g` on object files in a directory and reports duplicate
    /// defined text symbols that would cause linker errors.
    ///
    /// Examples:
    ///   tungsten doctor check link-collisions /tmp/tungsten_codegen/
    #[cfg(feature = "codegen")]
    LinkCollisions {
        /// Directory containing .o files to check
        dir: PathBuf,
    },

    /// Check mono ownership coverage for all TyApp sites (ADR 8.5.26i)
    ///
    /// Runs the mono discovery + ownership pipeline, then walks all term
    /// trees to verify every `TyApp(Global(name), ty)` has a corresponding
    /// entry in the frozen ownership map.
    ///
    /// Examples:
    ///   tungsten doctor check mono-coverage src/compiler/main.tg
    #[cfg(feature = "codegen")]
    MonoCoverage {
        /// The root source file to check
        file: PathBuf,
    },

    /// Detect foo.rs + foo/mod.rs coexistence (E0761 prevention)
    ///
    /// Walks Rust source directories and reports any file that has both
    /// a standalone .rs file and a directory module with mod.rs.
    /// Cost 1: filesystem walk only, no parsing or elaboration.
    ///
    /// Examples:
    ///   tungsten doctor check module-overlap
    ///   tungsten doctor check module-overlap --path tungsten_codegen/src
    ///   tungsten doctor check module-overlap --json
    ModuleOverlap {
        /// Directory to scan (replaces default roots: bootstrap/src/ + tungsten_codegen/src/)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Check Phase A.5 global collection health (ADR 13.5.26g §2.3)
    ///
    /// Runs Phase A.5 (build combined AST + global collection) and reports
    /// success or failure with source-level diagnostics. Cost 3 (elaboration).
    ///
    /// Examples:
    ///   tungsten doctor check phase-a5 src/compiler/main.tg
    ///   tungsten doctor check phase-a5 examples/list.tg
    #[command(name = "phase-a5")]
    PhaseA5 {
        /// The root source file to check
        file: PathBuf,
    },

    /// Verify compiled binary link health (ADR 19.5.26d)
    ///
    /// Checks that a compiled Tungsten binary has the expected properties:
    /// stack size, executability, and correct linker flags. Cost 1 (no elaboration).
    ///
    /// Examples:
    ///   tungsten doctor check link-health ./tungsten1
    ///   tungsten doctor check link-health ./tungsten1 -v
    LinkHealth {
        /// Path to the compiled binary to check
        binary: PathBuf,
    },

    /// Pre-flight checks for self-compile readiness (ADR 19.5.26d)
    ///
    /// Validates that the current platform and environment can successfully
    /// self-compile: filesystem case sensitivity, C compiler, linker capabilities,
    /// LLVM availability, and static library presence. Cost 1 (no elaboration).
    ///
    /// Examples:
    ///   tungsten doctor check self-compile-readiness
    ///   tungsten doctor check self-compile-readiness -v
    SelfCompileReadiness,

    /// Detect nested constructor+tuple match patterns (ADR 20.5.26a)
    ///
    /// Walks the AST and reports patterns of the form `Ctor((a, b))` where
    /// a constructor pattern contains a tuple subpattern. These patterns are
    /// known to cause "unknown value" errors in tungsten1. Cost 2 (parse only).
    ///
    /// Examples:
    ///   tungsten doctor check nested-patterns src/compiler/main.tg
    ///   tungsten doctor check nested-patterns src/compiler/main.tg -v
    #[command(name = "nested-patterns")]
    NestedPatterns {
        /// The root source file to check
        file: PathBuf,
    },

    // ── Hidden legacy aliases (ADR 12.5.26h §2.3) ──
    #[command(name = "normalization-consistency", hide = true)]
    NormalizationConsistencyLegacy { file: PathBuf },

    #[command(name = "encoding-depth", hide = true)]
    EncodingDepthLegacy(EncodingDepthArgs),

    #[command(name = "type-sizes", hide = true)]
    TypeSizesLegacy {
        file: PathBuf,
        #[arg(long, default_value_t = 5000)]
        max_nodes: usize,
    },

    #[command(name = "phase-invariants", hide = true)]
    PhaseInvariantsLegacy { file: PathBuf },

    #[command(name = "fold-consistency", hide = true)]
    FoldConsistencyLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "ir-layout", hide = true)]
    IrLayoutLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "stubs", hide = true)]
    StubsLegacy { file: PathBuf },

    #[command(name = "constructor-counts", hide = true)]
    ConstructorCountsLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "declares", hide = true)]
    DeclaresLegacy {
        #[arg(long)]
        from_existing_ir: PathBuf,
    },
}
