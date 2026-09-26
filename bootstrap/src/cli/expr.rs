//! `tungsten expr` — evaluate Tungsten given directly, rather than from a file
//! (ADR 19.8.26a).
//!
//! Grouped because the bootstrap's **top-level** namespace was at 14 of
//! `cli-surface`'s 15, and every previous capacity count had been taken inside
//! `doctor` — so the most exposed namespace in the repo was also the one
//! nobody was watching. Top-level is the worst place for that to be true: a new
//! subsystem's tooling is exactly what earns a new top-level namespace, so the
//! surface with the least headroom is the one whose next addition is least
//! likely to be small.
//!
//! `eval` and `repl` are the pair that came out, and the reason is prose cost
//! rather than cohesion alone — 15.8.26a's criterion, applied to a live-surface
//! grep of every top-level spelling:
//!
//! | command | mentions outside `notes/` |
//! |---|---:|
//! | `tungsten run` | 104 |
//! | `tungsten clean` | 6 |
//! | `tungsten eval` | 5 |
//! | `tungsten repl` | 4 |
//!
//! `run` would have made the group a clean "execute something" triple and cost
//! twenty times the churn of the whole ADR, so it stayed flat. What is left is
//! still one concern and not a leftovers bin: both members take their source
//! **as an argument or from a prompt**, never from a path — which is exactly
//! what separates them from `check` / `run` / `test` / `compile`, and why
//! neither has a `file` operand.
//!
//! The flat `tungsten eval <expr>` and `tungsten repl` spellings remain as
//! hidden aliases on [`super::Commands`], so no caller breaks.

use clap::Subcommand;

/// Expression-evaluation subcommands (ADR 19.8.26a).
///
/// Grouped under `tungsten expr <subcommand>`.
#[derive(Subcommand)]
pub(crate) enum ExprCommands {
    /// Evaluate an expression given on the command line
    ///
    /// Elaborates and evaluates a single expression with no source file
    /// involved. For a file, use `tungsten run <file>`.
    ///
    /// Examples:
    ///   tungsten expr eval "1 + 2"
    #[command(
        after_help = "See also: `tungsten expr repl` (the same evaluator, interactively), \
                            `tungsten run <file>`."
    )]
    Eval {
        /// The expression to evaluate
        expr: String,
    },

    /// Start an interactive REPL
    ///
    /// The same evaluator `expr eval` uses, reading expressions from a prompt
    /// instead of from `argv`.
    ///
    /// Examples:
    ///   tungsten expr repl
    #[command(
        after_help = "See also: `tungsten expr eval <expr>` (one expression, non-interactively)."
    )]
    Repl,
}
