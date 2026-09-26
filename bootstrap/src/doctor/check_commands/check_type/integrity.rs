//! `tungsten doctor check type integrity` — did elaboration leave the type
//! arena internally consistent? (ADR 15.8.26a).
//!
//! Grouped because `doctor check type` was at 13 of 15 and these four are the
//! subset the namespace's own concern table had already nominated. They share a
//! shape none of the remaining type checks has: each reports on **residue** —
//! something a completed elaboration should have finished with and did not —
//! rather than on a property of the types themselves. Nothing here can reject a
//! program; a finding says the pipeline left the arena in a state later phases
//! will misread, which is why they cluster with each other and not with the
//! admission gates (`positivity`, `termination`, `vacuous-mu`).
//!
//! | subcommand | the residue it looks for |
//! |---|---|
//! | `type-stubs` | type names still registered as stubs after full elaboration |
//! | `constructor-stubs` | encoded types / field types left as a raw `TyVar` naming a known ADT |
//! | `constructor-counts` | constructor lists whose five index/name invariants no longer hold |
//! | `phase-invariants` | invariants violated at an elaboration phase boundary |
//!
//! **`stubs` is spelled `type-stubs` here**, and that is the one rename rather
//! than a re-parenting. Flat, its neighbour `constructor-stubs` supplied the
//! missing noun by contrast; grouped, the two sit adjacent and `integrity
//! stubs` no longer says *which* stubs. The flat `check type stubs` spelling
//! remains, so the rename costs no caller anything.
//!
//! The flat `check type {stubs, constructor-stubs, constructor-counts,
//! phase-invariants}` spellings all remain as hidden aliases on
//! [`super::CheckTypeCommands`], as do the `check`-level ones this never
//! touched.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use crate::doctor::checks;

/// Elaboration-integrity check subcommands (ADR 15.8.26a).
///
/// Grouped under `tungsten doctor check type integrity <subcommand>`.
#[derive(Subcommand)]
pub enum CheckIntegrityCommands {
    /// Detect residual type stubs after elaboration (ADR 6.5.26a)
    ///
    /// Checks whether any registered type names remain as stubs after
    /// full elaboration.
    ///
    /// See also: `tungsten doctor check type integrity constructor-stubs
    /// <file>` (the same question one level down, about constructor metadata).
    ///
    /// Examples:
    ///   tungsten doctor check type integrity type-stubs examples/hello.tg
    #[command(name = "type-stubs")]
    TypeStubs {
        /// The source file to check
        file: PathBuf,
    },

    /// Detect stale constructor stubs after elaboration
    ///
    /// Checks that no ADT's encoded type or constructor field type is a
    /// raw TyVar matching a known ADT name. Stale stubs cause E0999 match
    /// dispatch failures in cross-module scenarios.
    ///
    /// See also: `tungsten doctor check type integrity constructor-counts
    /// <file>` (constructor-list integrity — the sibling constructor check).
    ///
    /// Examples:
    ///   tungsten doctor check type integrity constructor-stubs examples/list.tg
    ///   tungsten doctor check type integrity constructor-stubs src/compiler/main.tg
    #[command(name = "constructor-stubs")]
    ConstructorStubs {
        /// The source file to check
        file: PathBuf,
    },

    /// Validate constructor-list integrity for all ADTs (ADR 7.5.26e)
    ///
    /// Checks that each ADT's constructor list satisfies five invariants:
    /// entry count, unique indices, contiguous indices, unique names,
    /// and parent-type consistency.
    ///
    /// See also: `tungsten doctor check type integrity constructor-stubs
    /// <file>` (stale-stub detection — the sibling constructor check).
    ///
    /// Examples:
    ///   tungsten doctor check type integrity constructor-counts examples/list.tg
    #[command(name = "constructor-counts")]
    ConstructorCounts {
        /// The source file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Check elaboration phase invariants (ADR 20.4.26e)
    ///
    /// Runs the elaboration pipeline with invariant checks inserted at
    /// each phase boundary, reporting any violations.
    ///
    /// See also: `tungsten doctor check type integrity type-stubs <file>`
    /// (the residue a violated Stub-Registration invariant leaves behind).
    ///
    /// Examples:
    ///   tungsten doctor check type integrity phase-invariants examples/list.tg
    #[command(name = "phase-invariants")]
    PhaseInvariants {
        /// The source file to check
        file: PathBuf,
    },
}

/// Dispatch an elaboration-integrity check subcommand.
pub fn dispatch_check_integrity(cmd: CheckIntegrityCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckIntegrityCommands::TypeStubs { file } => {
            checks::check_stubs::cmd_check_stubs(&file, verbose, 20)
        }
        CheckIntegrityCommands::ConstructorStubs { file } => {
            checks::check_constructor_stubs::cmd_check_constructor_stubs(&file, verbose, 20)
        }
        CheckIntegrityCommands::ConstructorCounts { file, json } => {
            checks::check_constructor_counts::cmd_check_constructor_counts(&file, verbose, 20, json)
        }
        CheckIntegrityCommands::PhaseInvariants { file } => {
            checks::check_phase_invariants::cmd_check_phase_invariants(&file, verbose, 20)
        }
    }
}
