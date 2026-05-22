//! `tungsten doctor check type` — type-related health checks (ADR 12.5.26h).
//!
//! Groups 7 type-system health checks under `doctor check type ...`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};

use super::checks;

/// Shared arguments for `doctor check type encoding-depth` (ADR 12.5.26h).
#[derive(Args)]
pub struct EncodingDepthArgs {
    /// The source file to check
    pub file: PathBuf,

    /// Maximum encoding stack depth (SCC group size)
    #[arg(long, default_value_t = 20)]
    pub max_stack: usize,

    /// Maximum type-term tree depth
    #[arg(long, default_value_t = 50)]
    pub max_depth: usize,

    /// Maximum type-term node count
    #[arg(long, default_value_t = 5000)]
    pub max_nodes: usize,
}

/// Type-related health check subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten doctor check type <subcommand>`.
#[derive(Subcommand)]
pub enum CheckTypeCommands {
    /// Check normalization consistency across all types (ADR 20.4.26c)
    ///
    /// Re-elaborates the project and compares cached type encodings
    /// with fresh encodings to detect normalization divergences.
    ///
    /// Examples:
    ///   tungsten doctor check type normalization-consistency examples/list.tg
    NormalizationConsistency {
        /// The source file to check
        file: PathBuf,
    },

    /// Check encoding stack depth and type-term depth (ADR 20.4.26d)
    ///
    /// Reports maximum SCC group size (encoding stack depth), type-term
    /// tree depth, and type-term node count across all encoded types.
    ///
    /// Examples:
    ///   tungsten doctor check type encoding-depth examples/list.tg
    EncodingDepth(EncodingDepthArgs),

    /// Report node counts for all type encodings (ADR 20.4.26d)
    ///
    /// Lists all cached type encodings sorted by size (node count),
    /// and flags types exceeding a configurable threshold.
    ///
    /// Examples:
    ///   tungsten doctor check type type-sizes examples/list.tg
    TypeSizes {
        /// The source file to check
        file: PathBuf,

        /// Maximum type-term node count
        #[arg(long, default_value_t = 5000)]
        max_nodes: usize,
    },

    /// Check elaboration phase invariants (ADR 20.4.26e)
    ///
    /// Runs the elaboration pipeline with invariant checks inserted at
    /// each phase boundary, reporting any violations.
    ///
    /// Examples:
    ///   tungsten doctor check type phase-invariants examples/list.tg
    PhaseInvariants {
        /// The source file to check
        file: PathBuf,
    },

    /// Check fold/unfold consistency for all ADTs (ADR 21.4.26b)
    ///
    /// Validates that every ADT has consistent treatment across:
    /// SCC membership, μ-binder encoding, and Fold/Unfold in Core IR.
    ///
    /// Examples:
    ///   tungsten doctor check type fold-consistency examples/list.tg
    FoldConsistency {
        /// The source file to check
        file: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Detect residual type stubs after elaboration (ADR 6.5.26a)
    ///
    /// Checks whether any registered type names remain as stubs after
    /// full elaboration.
    ///
    /// Examples:
    ///   tungsten doctor check type stubs examples/hello.tg
    Stubs {
        /// The source file to check
        file: PathBuf,
    },

    /// Validate constructor-list integrity for all ADTs (ADR 7.5.26e)
    ///
    /// Checks that each ADT's constructor list satisfies five invariants:
    /// entry count, unique indices, contiguous indices, unique names,
    /// and parent-type consistency.
    ///
    /// Examples:
    ///   tungsten doctor check type constructor-counts examples/list.tg
    ConstructorCounts {
        /// The source file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Detect stale constructor stubs after elaboration
    ///
    /// Checks that no ADT's encoded type or constructor field type is a
    /// raw TyVar matching a known ADT name. Stale stubs cause E0999 match
    /// dispatch failures in cross-module scenarios.
    ///
    /// Examples:
    ///   tungsten doctor check type constructor-stubs examples/list.tg
    ///   tungsten doctor check type constructor-stubs src/compiler/main.tg
    ConstructorStubs {
        /// The source file to check
        file: PathBuf,
    },

    /// Detect inner foralls in structural type positions (ADR 21.5.26b)
    ///
    /// Scans all value definitions for Forall types embedded inside
    /// Sum/Product/Arrow — positions that require resolve_inner_foralls()
    /// before extract_type_arg_from_match can succeed.
    ///
    /// Examples:
    ///   tungsten doctor check type forall-resolution examples/list.tg
    ///   tungsten doctor check type forall-resolution src/compiler/main.tg
    ForallResolution {
        /// The source file to check
        file: PathBuf,
    },
}

/// Dispatch a type-related health check subcommand.
pub fn dispatch_check_type(cmd: CheckTypeCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckTypeCommands::NormalizationConsistency { file } => {
            checks::check_normalization::cmd_check_normalization_consistency(&file, verbose, 20)
        }
        CheckTypeCommands::EncodingDepth(args) => {
            let thresholds = checks::check_encoding_depth::DepthThresholds {
                max_stack: args.max_stack,
                max_depth: args.max_depth,
                max_nodes: args.max_nodes,
            };
            checks::check_encoding_depth::cmd_check_encoding_depth(
                &args.file,
                verbose,
                20,
                &thresholds,
            )
        }
        CheckTypeCommands::TypeSizes { file, max_nodes } => {
            checks::check_type_sizes::cmd_check_type_sizes(&file, verbose, 20, max_nodes)
        }
        CheckTypeCommands::PhaseInvariants { file } => {
            checks::check_phase_invariants::cmd_check_phase_invariants(&file, verbose, 20)
        }
        CheckTypeCommands::FoldConsistency { file, json } => {
            checks::check_fold_consistency::cmd_check_fold_consistency(&file, verbose, 20, json)
        }
        CheckTypeCommands::Stubs { file } => {
            checks::check_stubs::cmd_check_stubs(&file, verbose, 20)
        }
        CheckTypeCommands::ConstructorCounts { file, json } => {
            checks::check_constructor_counts::cmd_check_constructor_counts(&file, verbose, 20, json)
        }
        CheckTypeCommands::ConstructorStubs { file } => {
            checks::check_constructor_stubs::cmd_check_constructor_stubs(&file, verbose, 20)
        }
        CheckTypeCommands::ForallResolution { file } => {
            checks::check_forall_resolution::cmd_check_forall_resolution(&file, verbose, 20)
        }
    }
}
