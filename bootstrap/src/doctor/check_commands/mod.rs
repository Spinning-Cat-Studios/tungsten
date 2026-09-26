//! `tungsten doctor check` subcommand definitions (split from `doctor/mod.rs`
//! for the file-size convention when ADR 8.7.26a's `unit-cost` variant pushed
//! it past the threshold).
//!
//! Sub-namespaces:
//! - `doctor check type ...` — type-system checks (ADR 12.5.26h)
//! - `doctor check ir ...` — IR validation checks (ADR 12.5.26h)
//! - `doctor check link ...` — link-level checks (ADR 13.8.26c D5)
//! - `doctor check module ...` — module-and-name checks (ADR 29.8.26a D1)
//!
//! Legacy flat paths remain as hidden aliases at the end of the enum.

use std::path::PathBuf;

use clap::Subcommand;

#[cfg(feature = "codegen")]
pub(crate) mod check_codegen;
pub(crate) mod check_ir;
pub(crate) mod check_link;
pub(crate) mod check_module;
pub(crate) mod check_selfhost;
pub(crate) mod check_type;
mod help_text;

#[cfg(feature = "codegen")]
use check_codegen::CheckCodegenCommands;
use check_ir::CheckIrCommands;
use check_link::CheckLinkCommands;
use check_module::CheckModuleCommands;
use check_selfhost::CheckSelfhostCommands;
use check_type::{CheckTypeCommands, EncodingDepthArgs};

/// Health check subcommands grouped under `tungsten doctor check`.
///
/// Sub-namespaces:
/// - `doctor check type ...` — type-system checks (ADR 12.5.26h)
/// - `doctor check ir ...` — IR validation checks (ADR 12.5.26h)
/// - `doctor check link ...` — link-level checks (ADR 13.8.26c D5)
/// - `doctor check module ...` — module-and-name checks (ADR 29.8.26a D1)
///
/// **Both namespaces have headroom, and it was bought rather than granted.**
/// `doctor check` was at the `cli-surface` cap twice over: ADR 13.8.26c grouped
/// the two link checks under `check link` to make room for `name-collisions`,
/// and its review grouped the four codegen-pipeline checks under `check
/// codegen`, which is what took it off the cap rather than merely back to it.
/// `doctor check type` was likewise at 15, relieved by that same review's
/// `determinism` grouping and then by ADR 15.8.26a's `integrity` one. ADR
/// 29.8.26a bought the latest five slots by grouping the four module-and-name
/// checks, from 13 back to 10 — deliberately *before* the cap, because two
/// slots is the last point at which the grouping is voluntary rather than
/// forced on whoever adds the fifteenth check. Hidden aliases do not count, so
/// no move cost a caller anything.
///
/// Adding a variant to either namespace is therefore a design question, not a
/// free action: group a cohesive subset under a sub-namespace rather than
/// dropping a command, and prefer a group whose members already cross-reference
/// each other. Re-homing to `info` is **not** the cheap alternative it looks —
/// ADR 15.8.26a measured `info type` at 13 of 15, so moving two reporters there
/// would put that namespace at the cap instead.
///
/// Read the live counts with `code-health --check cli-surface`, never from
/// `--help`: the gate is a *source* scan counting every non-`hide = true`
/// variant and is blind to `#[cfg]`, so `--help` disagrees in both directions
/// (it adds a `help` row and omits codegen-gated variants).
#[derive(Subcommand)]
pub enum CheckCommands {
    // ── Visible grouped sub-namespaces ──
    /// Type-system health checks
    ///
    /// Two sub-namespaces plus the checks that answer a question of their own:
    /// `determinism` (elaborate twice and compare), `integrity` (what a
    /// completed elaboration left behind), and flat — the admission gates
    /// `positivity` / `termination` / `vacuous-mu`, plus the encoding-shape
    /// reporters.
    ///
    /// See also: `tungsten explain error NonStrictlyPositive` and
    /// `tungsten explain error E0062` for the two hard gates these mirror.
    #[command(subcommand)]
    Type(CheckTypeCommands),

    /// IR validation checks
    ///
    /// Sub-namespace for LLVM IR layout and declaration hygiene checks.
    /// See `tungsten doctor check ir --help` for details.
    #[command(subcommand)]
    Ir(CheckIrCommands),

    // ── Visible top-level checks ──
    /// Module-and-name health checks
    ///
    /// Sub-namespace for the four questions about how names and modules relate:
    /// what a module re-exports, which names collide, which modules overlap,
    /// and whether signature collection saw everything.
    /// See `tungsten doctor check module --help` for details.
    #[command(subcommand)]
    Module(CheckModuleCommands),

    /// Link-level health checks
    ///
    /// Sub-namespace for duplicate object-file symbols and compiled-binary
    /// link properties.
    /// See `tungsten doctor check link --help` for details.
    #[command(subcommand)]
    Link(CheckLinkCommands),

    /// Self-hosted-compiler health checks
    ///
    /// Sub-namespace for questions about what `tungsten1` produces rather than
    /// what this binary produces. Each spawns the self-host and reads its
    /// output, so each can also report that it could not be asked.
    #[command(subcommand)]
    Selfhost(CheckSelfhostCommands),

    /// Codegen-pipeline health checks
    ///
    /// Sub-namespace for checks that run the codegen pipeline: mono ownership,
    /// extern-map resolution, musttail coverage, per-unit cost.
    /// See `tungsten doctor check codegen --help` for details.
    #[cfg(feature = "codegen")]
    #[command(subcommand)]
    Codegen(CheckCodegenCommands),

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

    /// List every definition carrying a proof hole, and who put it there (ADR 18.9.26g)
    #[command(long_about = help_text::SORRY_SITES)]
    #[command(
        after_help = "See also: `tungsten doctor check nested-patterns` finds the pattern shapes \
                      the self-host miscompiles (cost 2); `tungsten info def <name> <file>` \
                      prints one definition's Core term (cost 3)."
    )]
    #[command(name = "sorry-sites")]
    SorrySites {
        /// The root source file to check
        file: PathBuf,
        /// Print the census as JSON
        #[arg(long)]
        json: bool,
    },

    /// Report `extern "C"` declarations the evaluator cannot execute (ADR 28.7.26a)
    ///
    /// The evaluator executes only allowlisted externs; a call to any other
    /// goes **silently Stuck** — no error, no output, the call simply never
    /// happens. This walks a file's declarations and flags the unsupported
    /// ones. Affects `run`/`test`/the playground only; native codegen links
    /// them fine. Cost 2 (parse only). Exit 2 on findings.
    ///
    /// Examples:
    ///   tungsten doctor check extern-coverage src/compiler/main.tg
    ///   tungsten doctor check extern-coverage tests/console_println_run.tg -v
    #[command(
        after_help = "See also: `tungsten info eval externs` lists the whole allowlist (cost 1)."
    )]
    #[command(name = "extern-coverage")]
    ExternCoverage {
        /// The root source file to check
        file: PathBuf,
    },

    /// Report whether the structural comparator can handle a type, and where
    /// it breaks (ADRs 1.8.26b, 1.8.26c)
    #[command(long_about = help_text::COMPARABLE)]
    #[command(
        after_help = "See also: `tungsten info type size <T> <file>` reports the μ-unfold factor, \
                      and `tungsten info type mu-members <T> <file>` shows what each μ-binder \
                      denotes (both cost 3)."
    )]
    #[command(name = "comparable")]
    Comparable {
        /// The type to check. With `--all`, omit it and pass only the file.
        type_name: Option<String>,
        /// The root source file declaring it. With `--all` this is the only
        /// positional argument.
        file: Option<PathBuf>,
        /// Check every type the file declares, in one elaboration.
        #[arg(long)]
        all: bool,
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

    // ── Hidden flat spellings for the `check codegen` group ──
    // Kept as the shape the binary-side dispatcher matches, so grouping these
    // four cost `main.rs` no edit at all: `check_codegen::flatten` rewrites the
    // grouped form onto them before dispatch. That matters beyond tidiness —
    // `main.rs` is the bin crate, whose `ExitCode`-returning dispatchers no
    // in-process test can assert on (9 of its 11 mutation sites survive on an
    // unmodified tree), so an edit there would have added permanently
    // unkillable mutants to the gate.
    #[cfg(feature = "codegen")]
    #[command(name = "mono-coverage", hide = true)]
    MonoCoverage {
        /// The root source file to check
        file: PathBuf,
    },

    #[cfg(feature = "codegen")]
    #[command(name = "extern-map-ambiguity", hide = true)]
    ExternMapAmbiguity {
        /// The root source file to check
        file: PathBuf,

        /// Emit machine-readable JSON instead of the human report.
        #[arg(long)]
        json: bool,
    },

    #[cfg(feature = "codegen")]
    #[command(name = "tco-coverage", hide = true)]
    TcoCoverage {
        /// The root source file to check (must contain a `main`).
        file: PathBuf,

        /// Emit machine-readable JSON instead of the table.
        #[arg(long)]
        json: bool,

        /// Filter to HIGH-risk rows only.
        #[arg(long = "risk", value_parser = ["high"])]
        risk: Option<String>,

        /// List per-call-site records instead of aggregated function rows.
        #[arg(long = "by-site")]
        by_site: bool,

        /// Also list EMIT/DECOMPOSE (LOW-risk) rows.
        #[arg(long)]
        emit: bool,

        /// Run as a deterministic CI gate (ADR 1.7.26e): exit non-zero on any
        /// un-allowlisted HIGH-risk SKIP, or any allowlist entry that participates
        /// in internal recursion. Consults `tools/tco-skip-allowlist.toml`.
        #[arg(long)]
        gate: bool,
    },

    #[cfg(feature = "codegen")]
    #[command(name = "unit-cost", hide = true)]
    UnitCost {
        /// The root source file to census (must contain a `main`).
        file: PathBuf,

        /// Emit machine-readable JSON instead of the table.
        #[arg(long)]
        json: bool,

        /// Cost bound: a time like '0.5s' or an allocation volume like '8GB'.
        /// Filters the report to units meeting it AND gates the exit code
        /// (non-zero when any unit meets it).
        #[arg(long)]
        threshold: Option<String>,

        /// Print the comma-separated TUNGSTEN_CODEGEN_SERIAL_UNITS value for
        /// units meeting the threshold (default 0.5s). Always exits 0.
        #[arg(long = "emit-serial-list")]
        emit_serial_list: bool,
    },

    // ── Hidden legacy aliases (ADR 13.8.26c D5) ──
    #[cfg(feature = "codegen")]
    #[command(name = "link-collisions", hide = true)]
    LinkCollisionsLegacy { dir: PathBuf },

    #[command(name = "link-health", hide = true)]
    LinkHealthLegacy { binary: PathBuf },

    // ── Hidden legacy aliases (ADR 29.8.26a D2) ──
    // The flat spellings of the four `check module` members. 87 references
    // across `.claude/`, `.github/`, `docs/repo-memory/`, `make/` and
    // `bootstrap/src/` name them, and keeping them resolvable is what makes the
    // surface sweep a documentation job rather than a flag day: a reference the
    // sweep missed still runs. `command-spellings` reports the ones still
    // outstanding.
    #[command(name = "reexport-completeness", hide = true)]
    ReexportCompletenessLegacy { file: PathBuf },

    #[command(name = "name-collisions", hide = true)]
    NameCollisionsLegacy {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = NameCollisionSeverity::All)]
        severity: NameCollisionSeverity,
        #[arg(long)]
        json: bool,
        #[arg(long = "include-reexports")]
        include_reexports: bool,
    },

    #[command(name = "module-overlap", hide = true)]
    ModuleOverlapLegacy {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },

    #[command(name = "signature-collection", hide = true)]
    SignatureCollectionLegacy { file: PathBuf },
}

/// `--severity` for `doctor check module name-collisions`, re-exported under a
/// self-documenting name. The check's own `Severity` derives `ValueEnum`, so
/// there is no second enum here to drift out of step with it.
pub use crate::doctor::checks::check_name_collisions::census::Severity as NameCollisionSeverity;
