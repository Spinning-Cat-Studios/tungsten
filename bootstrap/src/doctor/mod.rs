//! `tungsten doctor` — diagnostic namespace for compiler health checks.
//!
//! Sub-namespaces under `doctor check` (ADR 12.5.26h):
//! - `doctor check type ...` — type-system health checks
//! - `doctor check ir ...` — IR validation checks
//!
//! Legacy flat paths (e.g., `doctor check stubs`) remain as hidden aliases.
//! See ADR 16.4.26b for original design rationale.

pub mod audit_dead_definitions;
pub mod audit_driver_reach;
pub mod audit_mutual_types;
pub mod audit_orphan_sources;
pub mod audit_recursion;
mod check_commands;
pub mod checks;
pub mod diff_types;
mod dispatch;
/// Shared route-lowering probe (ADR 12.7.26c) — instantiates a `TypeLowering`,
/// so it is only present under the `codegen` feature. `pub` so the bin's
/// `info type lowering` can share it with the doctor check.
#[cfg(feature = "codegen")]
pub mod lowering_probe;
mod map_span;
mod module_overlap;
mod self_test;
pub(crate) mod suggest_tools;

#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

#[cfg(feature = "codegen")]
pub use check_commands::check_codegen::{flatten_codegen_check, CheckCodegenCommands};
pub use check_commands::check_ir::CheckIrCommands;
pub use check_commands::check_link::CheckLinkCommands;
pub use check_commands::check_module::CheckModuleCommands;
pub use check_commands::check_selfhost::CheckSelfhostCommands;
pub use check_commands::check_type::{CheckTypeCommands, EncodingDepthArgs};
pub use check_commands::CheckCommands;
pub use dispatch::cmd_doctor;
// Module re-exports so dispatch.rs's `use super::*` keeps resolving
// `check_ir::` / `check_type::` after the check_commands/ grouping.
pub(crate) use check_commands::{check_ir, check_link, check_module, check_selfhost, check_type};

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

        /// Skip the codegen consult; report the source-level estimate only
        /// (ADR 1.7.26b §2.2). Default consults codegen when the backend is
        /// available so the musttail verdict reflects the *actual* gate.
        #[arg(long = "source-only")]
        source_only: bool,
    },

    /// Census the definitions no entry point reaches (ADR 12.8.26b retrospective)
    ///
    /// Reachability from a declared root set, NOT an in-degree count: a
    /// mutually recursive pair nothing else calls has callers — each other —
    /// and is still dead.
    ///
    /// Roots are `main` and every `test_*` definition (the `tungsten test`
    /// discovery convention); `--root` adds more. The set is printed on every
    /// run, because a census that walked from the wrong roots is confidently
    /// wrong rather than visibly empty.
    ///
    /// Reports; never gates. Dead code is a finding to weigh.
    ///
    /// Examples:
    ///   tungsten doctor audit-dead-definitions src/compiler/main.tg
    ///   tungsten doctor audit-dead-definitions main.tg --root api_entry
    ///
    /// See also: `tungsten info def <name> <file> --callers` for one definition.
    AuditDeadDefinitions {
        /// The source file to analyze
        file: PathBuf,

        /// Treat this definition as an entry point (repeatable)
        #[arg(long = "root")]
        roots: Vec<String>,
    },

    /// Partition modules into driver-reached, test-only and unreached (ADR 3.9.26a)
    ///
    /// Answers "does the driver run this?" — a question about MODULES that
    /// spans ENTRY FILES, which `audit-dead-definitions` cannot express: it is
    /// per entry file and per definition, so it calls a test-only subsystem
    /// unreachable from `main.tg` and live from `test_codegen.tg`, and neither
    /// answer is the one you asked for.
    ///
    /// Reach is the module-level `use` graph, NOT the module tree: `main.tg`
    /// declares `mod codegen;` and no driver path imports it. Reaching
    /// `a::b::c` reaches `a::b` and `a`. Cost 2 (parse only) — the
    /// occurrence-graph route would cost one full elaboration per entry file.
    ///
    /// Test roots are the sibling `test_*.tg` / `mustfail_*.tg` entry files;
    /// `--test-entry` adds more. Reports, never gates: a module that is
    /// test-only today and driver-reached tomorrow is progress.
    ///
    /// Examples:
    ///   tungsten doctor audit-driver-reach src/compiler/main.tg
    ///   tungsten doctor audit-driver-reach main.tg --test-entry suites/extra.tg
    ///
    /// See also: `tungsten doctor audit-dead-definitions <file>` for the
    /// per-definition census, `tungsten info module tree <file>` for the
    /// declaration hierarchy this deliberately does not use.
    AuditDriverReach {
        /// The driver entry file to walk from
        file: PathBuf,

        /// Treat this file as an additional test entry point (repeatable)
        #[arg(long = "test-entry")]
        test_entries: Vec<PathBuf>,
    },

    /// Census the .tg files on disk that no module tree declares (ADR 3.9.26q)
    ///
    /// The filesystem MINUS the module tree, which is the one direction no
    /// other tool can walk: `tungsten check`, `audit-dead-definitions` and
    /// `audit-driver-reach` all start from an entry file and follow
    /// declarations, so a file no `mod` statement names is not in any of their
    /// starting sets. `code-health` does read the disk, and counts an orphan's
    /// lines against the size budgets while never asking whether anything
    /// reads it.
    ///
    /// Walks every `.tg` file under the entry file's directory, subtracts the
    /// modules declared by that file's tree and by each sibling `test_*.tg` /
    /// `mustfail_*.tg` tree, then splits the remainder three ways: STRANDED
    /// (nothing names it), BUILD-SWAPPED (a `make` recipe or script copies it
    /// into place — undeclared by design, not debt) and ENTRY FILES (roots, so
    /// nothing declares them and nothing should). Cost 2 (parse only).
    ///
    /// Reports, never gates: a file can be legitimately undeclared while it is
    /// being written, and whether a stranded file should be deleted or wired up
    /// is a judgement.
    ///
    /// Examples:
    ///   tungsten doctor audit-orphan-sources src/compiler/main.tg
    ///
    /// See also: `tungsten doctor audit-driver-reach <file>` partitions the
    /// modules that ARE declared, `tungsten doctor audit-dead-definitions
    /// <file>` the definitions inside them, `tungsten info module tree <file>`
    /// the declaration hierarchy this subtracts.
    AuditOrphanSources {
        /// The entry file whose directory is walked and whose tree is subtracted
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

    /// Check that each failure mode's companion diagnostic still reports (ADR 12.8.26a)
    ///
    /// Making a gate hard can make its own reporting tool unreachable: the tool
    /// elaborates the file before printing, so a gate that aborts elaboration
    /// deletes the diagnostic aimed at exactly the files it rejects. Nothing
    /// else notices — the subcommand still exists and still describes its old
    /// behaviour. Runs each (gate, companion) pairing against a fixture the gate
    /// rejects and fails when reality disagrees with what the pairing declares,
    /// **in either direction**. Cost 3 (elaborates small fixtures).
    ///
    /// A sibling of `suggest-tools` rather than a `check` subcommand: both ask
    /// about the diagnostic surface itself, not about a program.
    ///
    /// See also: `tungsten doctor check type termination`,
    /// `tungsten info def <name> <file> --why-not-certified`,
    /// `tungsten doctor check type positivity`,
    /// `tungsten doctor check type vacuous-mu`,
    /// `tungsten info type type-encoding <T>` — the paired companions. Run the
    /// command for the current table rather than counting them here: a figure
    /// in prose is the drift this check exists to catch.
    ///
    /// Examples:
    ///   tungsten doctor tool-reachability
    #[command(name = "tool-reachability")]
    ToolReachability,

    /// Suggest diagnostic tools for an error description (ADR 21.4.26d)
    ///
    /// Maps a free-text error description to ranked diagnostic commands.
    /// Uses keyword matching against a static pattern registry. Cost 1
    /// (no file I/O, no elaboration). Designed for AI agent consumption.
    ///
    /// Describe what you SAW. Each pattern carries the words of the message
    /// and the words of the observation (ADR 4.9.26d), so a description that
    /// names no cause reaches the same commands as one that does — which is
    /// the case the mandated first step exists for.
    ///
    /// Examples:
    ///   tungsten doctor suggest-tools "SIGSEGV when running compiled program"
    ///   tungsten doctor suggest-tools "reading a field gives back the wrong value"
    ///   tungsten doctor suggest-tools "type mismatch" --json
    ///   tungsten doctor suggest-tools "the compiler has printed nothing for ten minutes"
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
    ///   tungsten doctor check type integrity type-stubs examples/hello.tg
    ///   tungsten doctor check fold-consistency examples/list.tg
    ///   tungsten doctor check declares --from-existing-ir target/ll/
    ///
    /// See also: `tungsten info` for read-only inspection, `tungsten explain` for documentation.
    #[command(subcommand)]
    Check(CheckCommands),
}
