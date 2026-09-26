//! `tungsten doctor check selfhost` — health checks on what the SELF-HOSTED
//! compiler produces (ADR 19.8.26d retrospective).
//!
//! A sub-namespace of its own because these checks differ from every sibling
//! in one structural way: they cannot answer from this process. The question
//! is about `tungsten1`'s output, so each spawns it and reads what it said —
//! which means each also has to distinguish "clean" from "could not ask",
//! a distinction the in-process checks never face.
//!
//! It is deliberately not folded into `doctor check type`: these are not
//! type-system questions, and grouping by *which compiler is under test* is
//! what a reader arriving from a self-host divergence is actually looking for.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use crate::doctor::checks::check_selfhost_closed_terms::cmd_check_selfhost_closed_terms;
use crate::doctor::checks::check_selfhost_well_typed_terms::cmd_check_selfhost_well_typed_terms;

/// Self-hosted-compiler health checks.
///
/// Grouped under `tungsten doctor check selfhost <subcommand>`.
#[derive(Subcommand)]
pub enum CheckSelfhostCommands {
    /// Report self-hosted Core terms that are not closed (ADR 19.8.26d retrospective)
    ///
    /// A free VALUE variable in an elaborated body means the elaborator
    /// resolved that name through its own environment and never emitted the
    /// binding. The body still type-checks — that environment is in scope when
    /// the check runs — and the compiled path still works, because codegen
    /// rebuilds the binding from the pattern. What breaks is everything that
    /// reads the Core term: the evaluator gets stuck, and any analysis that
    /// reasons by following bindings concludes the wrong thing quietly. ADR
    /// 21.8.26a emitted the missing projections and this census has held
    /// `src/compiler/main.tg` at 0 open ever since — read the residual COUNT
    /// rather than the verdict, because the same defect sat at three lowering
    /// sites and each was named only by what survived fixing the last.
    ///
    /// Runs `tungsten1 check <file> --check-free-vars` and reads its census.
    /// Cost 3 (one self-hosted elaboration). Exit 1 on findings, exit 2 when
    /// the self-host could not be asked — a binary with diagnostics stubbed
    /// out (the production default), one older than the flag, or a run that
    /// examined nothing. Those are failures rather than passes on purpose:
    /// this check exists because a silent no-op read as a clean bill of health.
    ///
    /// See also: `tungsten doctor check selfhost well-typed-terms` — the
    /// sibling question, and the one a term that passes THIS check can still
    /// fail. `tungsten diff selfhost-core <def> <file>` — the same
    /// divergence for ONE definition, shown as two terms side by side, when
    /// you already suspect which one. `tungsten doctor check type termination`
    /// — the analysis that a non-closed term silently defeats.
    ///
    /// Examples:
    ///   tungsten doctor check selfhost closed-terms src/compiler/main.tg
    ///   tungsten doctor check selfhost closed-terms examples/list.tg -v
    #[command(name = "closed-terms")]
    ClosedTerms {
        /// The root source file to check with the self-hosted compiler
        file: PathBuf,

        /// Path to the tungsten1 (self-compiled) binary
        #[arg(long, default_value = "./tungsten1")]
        selfhost_binary: PathBuf,
    },

    /// Report self-hosted eliminators standing over the wrong former (ADR 3.9.26h)
    ///
    /// The sibling of `closed-terms`, and a separate check because they answer
    /// different questions. `closed-terms` asks whether every name is bound; it
    /// answers 0 of 2298 over `src/compiler/main.tg`, and BOTH defects that
    /// surfaced while closing ADR 21.8.26a pass it. A saturated constructor
    /// application elaborated to `App(App(λx:(A × B). …, 9), N2)` — closed, and
    /// applying a unary lambda's RESULT to a second argument. A tuple
    /// projection emitted `fst` of a scalar. Codegen never reads the Core term
    /// and the type checker resolves through an environment still in scope, so
    /// nothing else in the repo can see either.
    ///
    /// Shape agreement, not typing: whether an eliminator's operand has the
    /// right former — `Fst`/`Snd` over a Product, `App` over an arrow, `Case`
    /// over a Sum, `Unfold` over a Mu. Where the elaborator recorded no type
    /// there is no finding.
    ///
    /// Runs `tungsten1 check <file> --check-well-typed` and reads its census.
    /// Cost 3 (one self-hosted elaboration). Exit 2 when the self-host could
    /// not be asked — a binary with diagnostics stubbed out (the production
    /// default), one older than the flag, or a run that examined nothing.
    ///
    /// Over `src/compiler/main.tg` it is a SHRINK-ONLY baseline, not a hard 0:
    /// ADR 3.9.26h measured 300 of 2302 before either motivating defect was
    /// fixed, so exit 1 means either MORE than that (a regression) or FEWER
    /// (lower `MAIN_TG_BASELINE`, so the gain cannot be given back). Every
    /// other corpus is gated at 0 — a baseline belongs to the corpus it was
    /// measured on.
    ///
    /// See also: `tungsten doctor check selfhost closed-terms` — the other
    /// half; run both, because a term can fail either alone. `tungsten1 run
    /// <file>` — what an ill-shaped term looks like without this check: an
    /// unreduced term and the words "not a value".
    ///
    /// Examples:
    ///   tungsten doctor check selfhost well-typed-terms src/compiler/main.tg
    ///   tungsten doctor check selfhost well-typed-terms examples/list.tg -v
    #[command(name = "well-typed-terms")]
    WellTypedTerms {
        /// The root source file to check with the self-hosted compiler
        file: PathBuf,

        /// Path to the tungsten1 (self-compiled) binary
        #[arg(long, default_value = "./tungsten1")]
        selfhost_binary: PathBuf,
    },
}

/// Dispatch `doctor check selfhost <subcommand>`.
pub fn dispatch_check_selfhost(command: CheckSelfhostCommands, verbose: bool) -> ExitCode {
    match command {
        CheckSelfhostCommands::ClosedTerms {
            file,
            selfhost_binary,
        } => cmd_check_selfhost_closed_terms(&file, &selfhost_binary, verbose),
        CheckSelfhostCommands::WellTypedTerms {
            file,
            selfhost_binary,
        } => cmd_check_selfhost_well_typed_terms(&file, &selfhost_binary, verbose),
    }
}
