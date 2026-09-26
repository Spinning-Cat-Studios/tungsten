//! `tungsten doctor check type` — type-related health checks (ADR 12.5.26h).
//!
//! Two sub-namespaces carry the checks that share a shape, so this enum holds
//! only the ones that answer a question of their own:
//!
//! - `determinism` — elaborate twice and compare (ADR 13.8.26c review)
//! - `integrity` — what a completed elaboration left behind (ADR 15.8.26a)
//!
//! Every flat spelling either grouping replaced survives as a hidden alias at
//! the tail of [`CheckTypeCommands`]. Hidden variants are exempt from the
//! `cli-surface` count, so keeping every existing caller working is free.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::doctor::checks;

use clap::{Args, Subcommand};

// The two sub-namespaces are this namespace's children rather than its
// siblings (ADR 29.8.26a's close-out): `check_commands/` reached its
// directory-file threshold when `check_module.rs` landed, and the cohesive
// subset to group was the one that was already a tree at the CLI level.
pub(crate) mod determinism;
pub(crate) mod integrity;

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
    /// Determinism checks
    ///
    /// Sub-namespace for the three checks that elaborate twice (or re-derive)
    /// and compare: is the stored encoding map stable, is the resolution work
    /// stable, does a fresh derivation agree with the cache?
    /// See `tungsten doctor check type determinism --help` for details.
    #[command(subcommand)]
    Determinism(determinism::CheckDeterminismCommands),

    /// Elaboration-integrity checks
    ///
    /// Did elaboration leave the type arena internally consistent? Four checks
    /// answer it from different angles — residual type stubs, stale constructor
    /// metadata, constructor-list invariants, phase-boundary invariants — and
    /// each reports *residue*, something a finished pipeline should not have
    /// left. None can reject a program; each reports state a later phase will
    /// misread. Contrast the admission gates (`positivity`, `termination`,
    /// `vacuous-mu`), which decide whether a program is legal at all.
    #[command(subcommand)]
    Integrity(integrity::CheckIntegrityCommands),

    /// Check strict positivity of every type definition (ADR 7.8.26e)
    ///
    /// Runs the same engine as the E0061 gate, so the two cannot disagree —
    /// but it elaborates the project first. A file the gate REJECTS therefore
    /// exits 2 (elaboration failed) and never reaches this verdict: to
    /// diagnose a rejection, read the E0061 message, which names the type,
    /// constructor, field and inherited-through chain.
    ///
    /// Reach for this on a corpus that compiles. It reports each parameter's
    /// computed occurrence (`unused` / `strict` / `forbidden`, `--verbose`),
    /// the largest SCC and Tarjan depth of the expanded type graph, and the
    /// `App`/`Adt` heads that could not be resolved (split by lossy stub vs
    /// genuinely absent).
    ///
    /// See also: `tungsten explain error NonStrictlyPositive` (what the rule
    /// is and how to fix a rejection), `tungsten doctor check type
    /// encoding-depth <file>` (stack / tree-depth metrics).
    ///
    /// Examples:
    ///   tungsten doctor check type positivity examples/list.tg
    ///   tungsten doctor check type positivity src/compiler/main.tg --verbose
    Positivity {
        /// The source file to check
        file: PathBuf,
    },

    /// Find types whose encoding collapsed to a vacuous `μX. X` (ADR 11.8.26c)
    ///
    /// A *nested* inductive family (`type Rose = Node(Wrap<Rose>)`) has nowhere
    /// to put its recursion, so it encodes as a binder whose body is the binder
    /// — and every `match` on it is rejected E0064. The definition is accepted
    /// and checks clean, so a project carries the shape invisibly until the
    /// first `match` is written; this finds it before then, and is reachable
    /// where `info type type-encoding` is not (E0064 blocks that on a file
    /// already matching on one). Exits non-zero naming each type.
    ///
    /// See also: `tungsten explain error NestedRecursiveFamily`,
    /// `tungsten info type type-encoding <T> <file>`.
    /// Example: tungsten doctor check type vacuous-mu src/compiler/main.tg
    #[command(name = "vacuous-mu")]
    VacuousMu {
        /// The source file to check
        file: PathBuf,
    },

    /// Report Phase-1 structural-recursion admission (ADR 29.6.26e)
    ///
    /// Termination's sibling soundness gate to `positivity`: strict positivity
    /// makes "strict subterm" well-founded, and this checks that recursion
    /// actually descends along it. Runs the E0062/E0063 gate's own engine and
    /// prints the census the gate does not — which recursive groups were
    /// certified, which were admitted opaquely through `#[partial]` taint, and
    /// which could not be admitted, each with the decreasing parameter and the
    /// offending argument. Exit 1 on a non-admitted definition.
    ///
    /// Unlike `positivity`, this **is** reachable on a file the gate rejects —
    /// and since ADR 11.8.26b made termination a hard gate, that is no longer
    /// automatic: it forces `Report` enforcement for its own elaboration (ADR
    /// 12.8.26a), so the census still prints where the build would abort. The
    /// exit code comes from the census, not from enforcement, so CI can still
    /// fail on it.
    ///
    /// See also: `tungsten explain error E0062` (the rule),
    /// `tungsten info def <name> <file> --why-not-certified` (why one
    /// definition's parameters were refused as decreasing roots),
    /// `--termination proofs` (demote executable rejections to warnings),
    /// `tungsten doctor tool-reachability` (keeps this promise honest).
    ///
    /// Examples:
    ///   tungsten doctor check type termination examples/list.tg
    ///   tungsten doctor check type termination src/compiler/main.tg --verbose
    Termination {
        /// The root source file to check
        file: PathBuf,
    },

    /// Check encoding stack depth and type-term depth (ADR 20.4.26d)
    ///
    /// Reports maximum SCC group size (encoding stack depth), type-term
    /// tree depth, and type-term node count across all encoded types.
    ///
    /// See also: `tungsten doctor check type type-sizes <file>` (per-type
    /// node-count ranking).
    ///
    /// Examples:
    ///   tungsten doctor check type encoding-depth examples/list.tg
    EncodingDepth(EncodingDepthArgs),

    /// Report node counts for all type encodings (ADR 20.4.26d)
    ///
    /// Lists all cached type encodings sorted by size (node count),
    /// and flags types exceeding a configurable threshold.
    ///
    /// See also: `tungsten doctor check type encoding-depth <file>` (stack /
    /// tree-depth metrics).
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

    /// Check that every ADT lowers identically via every route (ADR 12.7.26c)
    ///
    /// The regression gate for the named-vs-structural split-brain: for each
    /// non-recursive ADT, compares the LLVM layout produced by every lowering
    /// route (named / app / structural / flat-adt) and reports any divergence.
    /// Requires the `codegen` feature; exits non-zero on a divergence.
    ///
    /// See also: `tungsten info type lowering <name> <file>` (per-type lens),
    /// `tungsten diff ir` (structural IR comparison).
    ///
    /// Examples:
    ///   tungsten doctor check type lowering-consistency tests/comparator_codegen_run.tg
    ///   tungsten doctor check type lowering-consistency src/compiler/main.tg --json
    #[cfg(feature = "codegen")]
    LoweringConsistency {
        /// The source file to check
        file: PathBuf,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    // ── Hidden legacy aliases (ADR 13.8.26c review) ──
    // The flat spellings the `determinism` grouping replaced. Exempt from the
    // `cli-surface` count, so keeping every existing caller working is free.
    #[command(name = "normalization-consistency", hide = true)]
    NormalizationConsistencyLegacy {
        file: PathBuf,
        #[arg(long)]
        raw_only: bool,
    },

    #[command(name = "encoding-determinism", hide = true)]
    EncodingDeterminismLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "resolution-attempt-determinism", hide = true)]
    ResolutionAttemptDeterminismLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    // ── Hidden legacy aliases (ADR 15.8.26a) ──
    // The flat spellings the `integrity` grouping replaced. `stubs` is the one
    // that also changed name (to `integrity type-stubs`), so this alias is both
    // a re-parenting and a rename — which is exactly why it is kept.
    #[command(name = "stubs", hide = true)]
    StubsLegacy { file: PathBuf },

    #[command(name = "constructor-stubs", hide = true)]
    ConstructorStubsLegacy { file: PathBuf },

    #[command(name = "constructor-counts", hide = true)]
    ConstructorCountsLegacy {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "phase-invariants", hide = true)]
    PhaseInvariantsLegacy { file: PathBuf },
}

/// Dispatch a type-related health check subcommand.
pub fn dispatch_check_type(cmd: CheckTypeCommands, verbose: bool) -> ExitCode {
    match cmd {
        CheckTypeCommands::Determinism(sub) => {
            determinism::dispatch_check_determinism(sub, verbose)
        }
        CheckTypeCommands::Integrity(sub) => integrity::dispatch_check_integrity(sub, verbose),
        // The flat aliases resolve to the same values, so there is one handler.
        CheckTypeCommands::NormalizationConsistencyLegacy { file, raw_only } => {
            determinism::dispatch_check_determinism(
                determinism::CheckDeterminismCommands::Normalization { file, raw_only },
                verbose,
            )
        }
        CheckTypeCommands::EncodingDeterminismLegacy { file, json } => {
            determinism::dispatch_check_determinism(
                determinism::CheckDeterminismCommands::Encoding { file, json },
                verbose,
            )
        }
        CheckTypeCommands::ResolutionAttemptDeterminismLegacy { file, json } => {
            determinism::dispatch_check_determinism(
                determinism::CheckDeterminismCommands::ResolutionAttempts { file, json },
                verbose,
            )
        }
        CheckTypeCommands::Positivity { file } => {
            checks::check_positivity::cmd_check_positivity(&file, verbose, 20)
        }
        CheckTypeCommands::VacuousMu { file } => {
            checks::check_vacuous_mu::cmd_check_vacuous_mu(&file, verbose, 20)
        }
        CheckTypeCommands::Termination { file } => {
            checks::check_termination::cmd_check_termination(&file, verbose, 20)
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
        CheckTypeCommands::FoldConsistency { file, json } => {
            checks::check_fold_consistency::cmd_check_fold_consistency(&file, verbose, 20, json)
        }
        // The flat aliases resolve to the same values, so there is one handler.
        CheckTypeCommands::StubsLegacy { file } => integrity::dispatch_check_integrity(
            integrity::CheckIntegrityCommands::TypeStubs { file },
            verbose,
        ),
        CheckTypeCommands::ConstructorStubsLegacy { file } => integrity::dispatch_check_integrity(
            integrity::CheckIntegrityCommands::ConstructorStubs { file },
            verbose,
        ),
        CheckTypeCommands::ConstructorCountsLegacy { file, json } => {
            integrity::dispatch_check_integrity(
                integrity::CheckIntegrityCommands::ConstructorCounts { file, json },
                verbose,
            )
        }
        CheckTypeCommands::PhaseInvariantsLegacy { file } => integrity::dispatch_check_integrity(
            integrity::CheckIntegrityCommands::PhaseInvariants { file },
            verbose,
        ),
        CheckTypeCommands::ForallResolution { file } => {
            checks::check_forall_resolution::cmd_check_forall_resolution(&file, verbose, 20)
        }
        #[cfg(feature = "codegen")]
        CheckTypeCommands::LoweringConsistency { file, json } => {
            checks::check_lowering_consistency::cmd_check_lowering_consistency(
                &file, verbose, 20, json,
            )
        }
    }
}
