//! `tungsten doctor check type determinism` — the checks that elaborate twice
//! and compare (ADR 13.8.26c review).
//!
//! Grouped because `doctor check type` was at 15 of 15 and these three are the
//! subset the namespace's own note had already nominated. They share a shape no
//! other type check has: each runs the pipeline **more than once** — or against
//! the cache — and reports on whether the two agree, rather than on what a
//! single run produced. The distinctions between them are worth keeping in view,
//! which the flat spellings buried:
//!
//! | subcommand | compares |
//! |---|---|
//! | `normalization` | the stored encodings against a fresh re-derivation |
//! | `encoding` | two runs' stored maps, structurally |
//! | `resolution-attempts` | two runs' deferred-resolution *work*, not results |
//!
//! The flat `check type {normalization-consistency, encoding-determinism,
//! resolution-attempt-determinism}` spellings remain as hidden aliases on
//! [`super::CheckTypeCommands`], so no caller breaks.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use crate::doctor::checks;

/// Determinism check subcommands (ADR 13.8.26c review).
///
/// Grouped under `tungsten doctor check type determinism <subcommand>`.
#[derive(Subcommand)]
pub enum CheckDeterminismCommands {
    /// Check normalization consistency across all types (ADR 20.4.26c)
    ///
    /// Re-elaborates the project and compares cached type encodings
    /// with fresh encodings to detect normalization divergences.
    ///
    /// See also: `tungsten doctor check type determinism encoding <file>`
    /// (is the stored map stable across runs?).
    ///
    /// Examples:
    ///   tungsten doctor check type determinism normalization examples/list.tg
    ///   tungsten doctor check type determinism normalization src/compiler/main.tg --raw-only
    #[command(name = "normalization")]
    Normalization {
        /// The source file to check
        file: PathBuf,

        /// Compare with raw structural `==` only, skipping the tier-2
        /// normalization fallback (ADR 22.7.26c). Tier-1 suffices on healthy
        /// code since the deterministic Phase-1d resolution order
        /// (ADR 22.7.26d); use as a regression canary for inline-depth
        /// instability — a raw-only divergence is a bug, not noise.
        #[arg(long)]
        raw_only: bool,
    },

    /// Check that the stored Phase-1e encoding map is stable across runs
    /// (ADR 22.7.26c close-out)
    ///
    /// Elaborates the project twice and compares the two `encoded_types` maps
    /// with strict structural `==` (NOT normalization). Each run seeds its
    /// `HashMap`s differently, so the two exercise different iteration orders
    /// — the axis ADR 22.7.26c made irrelevant to the stored output. Exits
    /// non-zero on any divergence.
    ///
    /// See also: `tungsten doctor check type determinism normalization <file>`
    /// (stored vs a fresh re-derivation). For the strongest cross-process
    /// guarantee, run `--json` in two processes and `diff`.
    ///
    /// Examples:
    ///   tungsten doctor check type determinism encoding examples/list.tg
    ///   tungsten doctor check type determinism encoding src/compiler/main.tg --json
    #[command(name = "encoding")]
    Encoding {
        /// The source file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Check that deferred type-reference resolution makes a stable *number*
    /// of attempts across runs (ADR 23.7.26a §6.1)
    ///
    /// The work-side complement of `encoding-determinism`: elaborates the
    /// project twice and compares the per-target-name count of deferred
    /// resolution attempts made by `resolve_deferred_type_references`
    /// (including the no-op re-resolutions §6.1 saw flap across processes while
    /// the stored encodings stayed byte-identical). Also prints the attempt
    /// counts (`--verbose`), so it doubles as an attempt counter. Exits
    /// non-zero on any divergence.
    ///
    /// See also: `tungsten doctor check type determinism encoding <file>`
    /// (are the stored *results* stable?). For the strongest cross-process
    /// guarantee, run `--json` in two processes and `diff`.
    ///
    /// Examples:
    ///   tungsten doctor check type determinism resolution-attempts examples/list.tg
    ///   tungsten doctor check type determinism resolution-attempts src/compiler/main.tg --verbose
    #[command(name = "resolution-attempts")]
    ResolutionAttempts {
        /// The source file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Dispatch a determinism check subcommand.
pub fn dispatch_check_determinism(cmd: CheckDeterminismCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckDeterminismCommands::Normalization { file, raw_only } => {
            checks::check_normalization::cmd_check_normalization_consistency(
                &file, verbose, 20, raw_only,
            )
        }
        CheckDeterminismCommands::Encoding { file, json } => {
            checks::check_normalization::cmd_check_encoding_determinism(&file, verbose, 20, json)
        }
        CheckDeterminismCommands::ResolutionAttempts { file, json } => {
            checks::check_normalization::cmd_check_resolution_attempt_determinism(
                &file, verbose, 20, json,
            )
        }
    }
}
