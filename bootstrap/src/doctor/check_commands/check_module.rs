//! `tungsten doctor check module` — the module-and-name layer (ADR 29.8.26a D1).
//!
//! Four checks that ask one question from four sides: what a module
//! **re-exports**, which names **collide**, which modules **overlap** on disk,
//! and whether **signature collection** saw everything. They already
//! cross-reference each other — `reexport-completeness` is `name-collisions`'
//! dual, and a signature-collection fault is usually a bad import in another
//! module — so a reader who wants one of them usually wants the neighbours too.
//!
//! **The membership test is the QUESTION, not the code.** A check that merely
//! reads module data does not join: `nested-patterns` walks patterns and
//! `extern-coverage` asks about the evaluator, and admitting them would make
//! this namespace mean "checks that touch modules", which is to say nothing.
//!
//! The grouping is not cosmetic either. `doctor check` stood at 13 of the 15
//! `cli-surface` allows, and the last two slots are the point at which
//! sub-namespacing is still cheap and voluntary rather than forced on whoever
//! adds the fifteenth check while trying to ship something else. This takes it
//! to 10.
//!
//! `doctor check reexport-completeness`, `name-collisions`, `module-overlap`
//! and `signature-collection` remain as hidden aliases in
//! [`super::CheckCommands`], so no caller and no `make` recipe breaks — and a
//! reference this ADR's sweep missed is a stale document rather than a broken
//! command. `code-health --check command-spellings` is what finds those.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use super::NameCollisionSeverity;
use crate::doctor::{checks, module_overlap};

/// Module-and-name health check subcommands (ADR 29.8.26a D1).
///
/// Grouped under `tungsten doctor check module <subcommand>`.
#[derive(Subcommand)]
pub enum CheckModuleCommands {
    /// Check pub use re-export completeness (ADR 8.5.26f)
    ///
    /// Walks the module tree and checks that every `pub use` declaration
    /// actually copied items. Reports declarations that resolved to zero
    /// items or had missing named imports.
    ///
    /// See also: `tungsten doctor check module name-collisions` — the dual
    /// question, whether one definition is reachable by several paths.
    ///
    /// Examples:
    ///   tungsten doctor check module reexport-completeness src/compiler/main.tg
    #[command(name = "reexport-completeness")]
    ReexportCompleteness {
        /// The root source file to check
        file: PathBuf,
    },

    /// Report value names defined in more than one reachable module
    /// (ADR 13.8.26c)
    #[command(long_about = super::help_text::NAME_COLLISIONS)]
    #[command(
        after_help = "See also: `tungsten doctor check module reexport-completeness` (is one \
                      definition reachable by a path? — this check's dual), `tungsten doctor check \
                      codegen extern-map-ambiguity` (which callee an emitted unit picks; needs \
                      successful elaboration and the codegen feature)."
    )]
    #[command(name = "name-collisions")]
    NameCollisions {
        /// The root source file whose reachable module tree to check
        file: PathBuf,

        /// Which classes to report: `all`, or `live` for the classes that are
        /// errors today (extern-symbol, private-shadowed).
        #[arg(long, value_enum, default_value_t = NameCollisionSeverity::All)]
        severity: NameCollisionSeverity,

        /// Emit machine-readable JSON instead of the human report.
        #[arg(long)]
        json: bool,

        /// Measurement only (ADR 13.8.26c AC 1): do not subtract the copies the
        /// `pub use` pass synthesizes. Every extra finding is one definition
        /// reachable by several paths — a re-export chain, not a collision — so
        /// the difference between the two runs IS the re-export class.
        #[arg(long = "include-reexports")]
        include_reexports: bool,
    },

    /// Detect foo.rs + foo/mod.rs coexistence (E0761 prevention)
    ///
    /// Walks Rust source directories and reports any file that has both
    /// a standalone .rs file and a directory module with mod.rs.
    /// Cost 1: filesystem walk only, no parsing or elaboration.
    ///
    /// Examples:
    ///   tungsten doctor check module overlap
    ///   tungsten doctor check module overlap --path tungsten_codegen/src
    ///   tungsten doctor check module overlap --json
    Overlap {
        /// Directory to scan (replaces default roots: bootstrap/src/ + tungsten_codegen/src/)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Check Signature Collection global collection health (ADR 13.5.26g §2.3)
    ///
    /// Runs Signature Collection (build combined AST + global collection) and reports
    /// success or failure with source-level diagnostics. Cost 3 (elaboration).
    ///
    /// Examples:
    ///   tungsten doctor check module signature-collection src/compiler/main.tg
    ///   tungsten doctor check module signature-collection examples/list.tg
    #[command(name = "signature-collection")]
    SignatureCollection {
        /// The root source file to check
        file: PathBuf,
    },
}

/// Dispatch a module-and-name health check subcommand.
pub fn dispatch_check_module(cmd: CheckModuleCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckModuleCommands::ReexportCompleteness { file } => {
            checks::check_reexport_completeness::cmd_check_reexport_completeness(&file, verbose)
        }
        CheckModuleCommands::NameCollisions {
            file,
            severity,
            json,
            include_reexports,
        } => checks::check_name_collisions::cmd_check_name_collisions(
            &file,
            severity,
            json,
            include_reexports,
        ),
        CheckModuleCommands::Overlap { path, json } => {
            module_overlap::cmd_check_module_overlap(path.as_deref(), json)
        }
        CheckModuleCommands::SignatureCollection { file } => {
            checks::check_signature_collection::cmd_check_signature_collection(&file, verbose)
        }
    }
}

// Tests: ../cli_tests/grouping.rs (the grouped spellings parse),
// ../cli_tests/aliases.rs (each flat spelling still resolves, flags and all),
// ../cli_tests/name_collisions.rs (what the flags resolve TO).
