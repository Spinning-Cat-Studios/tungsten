//! `tungsten doctor check link` — link-level health checks (ADR 13.8.26c D5).
//!
//! Groups the two checks about *linking* — duplicate object-file symbols and
//! the properties of a produced binary — under one namespace. The grouping is
//! not cosmetic: `doctor check` was at 15 of 15 subcommands, and `cli-surface`
//! fires above the cap, so a new sibling needed a slot. Sub-namespacing is the
//! remedy `cli-surface`'s own message asks for; dropping a command is the move
//! it warns against.
//!
//! `doctor check link-collisions` and `doctor check link-health` remain as
//! hidden aliases in [`super::CheckCommands`], so no caller and no `make`
//! recipe breaks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Subcommand;

use crate::doctor::checks;

/// Link-level health check subcommands (ADR 13.8.26c D5).
///
/// Grouped under `tungsten doctor check link <subcommand>`.
#[derive(Subcommand)]
pub enum CheckLinkCommands {
    /// Check for symbol collisions across object files (ADR 6.5.26d §2.5)
    ///
    /// Runs `nm -g` on object files in a directory and reports duplicate
    /// defined text symbols that would cause linker errors.
    ///
    /// See also: `tungsten doctor check module name-collisions` (the same duplicate,
    /// one phase earlier and without needing a build).
    ///
    /// Examples:
    ///   tungsten doctor check link collisions /tmp/tungsten_codegen/
    #[cfg(feature = "codegen")]
    Collisions {
        /// Directory containing .o files to check
        dir: PathBuf,
    },

    /// Report `extern "C"` declarations no tungsten_core symbol provides (ADR 18.8.26b retrospective)
    ///
    /// An `extern "C"` DECLARATION type-checks on every target — the symbol it
    /// names is not looked for until something links. So a `.tg` binding with
    /// no Rust side gives a clean `check`, a clean `cargo test`, a clean
    /// `check-health`, and then an `undefined reference` minutes into a
    /// self-compile. This walks a module tree's declarations and a source root's
    /// `#[no_mangle] extern "C"` exports and reports the ones that resolve to
    /// nothing. Cost 2 (two source walks). Exit 2 on findings.
    ///
    /// See also: `tungsten doctor check extern-coverage` — the same declarations,
    /// asked whether the EVALUATOR can execute them (a `run`/`test` question)
    /// rather than whether the linker can find them. The answers are
    /// independent: most of `main.tg`'s externs are unexecutable AND perfectly
    /// linkable.
    ///
    /// Examples:
    ///   tungsten doctor check link extern-symbols src/compiler/main.tg
    ///   tungsten doctor check link extern-symbols src/compiler/main.tg -v
    ExternSymbols {
        /// The root source file whose module tree is scanned for declarations
        file: PathBuf,

        /// Source root scanned for `#[no_mangle] extern "C"` exports
        #[arg(long, default_value = "tungsten_core/src")]
        core_root: PathBuf,
    },

    /// Verify compiled binary link health (ADR 19.5.26d)
    ///
    /// Checks that a compiled Tungsten binary has the expected properties:
    /// stack size, executability, and correct linker flags. Cost 1 (no elaboration).
    ///
    /// Examples:
    ///   tungsten doctor check link health ./tungsten1
    ///   tungsten doctor check link health ./tungsten1 -v
    Health {
        /// Path to the compiled binary to check
        binary: PathBuf,
    },
}

/// Dispatch a link-level health check subcommand.
pub fn dispatch_check_link(cmd: CheckLinkCommands, verbose: bool) -> ExitCode {
    match cmd {
        #[cfg(feature = "codegen")]
        CheckLinkCommands::Collisions { dir } => {
            checks::check_link_collisions::cmd_check_link_collisions(&dir)
        }
        CheckLinkCommands::ExternSymbols { file, core_root } => {
            checks::check_extern_symbols::cmd_check_extern_symbols(
                &file,
                Path::new(&core_root),
                verbose,
            )
        }
        CheckLinkCommands::Health { binary } => {
            checks::check_link_health::cmd_check_link_health(&binary, verbose)
        }
    }
}
