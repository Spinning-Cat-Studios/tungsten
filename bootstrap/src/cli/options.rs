//! Global CLI options: the top-level `Cli` parser and the value enums its
//! flags take.
//!
//! Split from `cli::mod` (which owns the `Commands` subcommand tree) on the
//! seam between *how a run is configured* and *what it does* — the two grow
//! independently, and `mod.rs` was one line from the file-size cap.

use clap::Parser;
use std::path::PathBuf;

use super::Commands;

/// Controls when ANSI color codes are emitted in test output.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ColorMode {
    /// Color when stdout is a TTY (default)
    Auto,
    /// Always emit color codes
    Always,
    /// Never emit color codes
    Never,
}

#[derive(Parser)]
#[command(name = "tungsten")]
#[command(author, version, about = "The Tungsten proof language compiler")]
#[command(
    long_about = "Tungsten is a proof language that combines programming and theorem proving.\n\n\
                  This is the bootstrap compiler, written in Rust. Once Tungsten is self-hosting,\n\
                  it will be replaced by a compiler written in Tungsten itself."
)]
#[command(
    after_help = "Core commands: check, run, test, compile, eval, repl, clean, cache\n\
                  Diagnostics:   info, explain, doctor, diff, commands\n\
                  Experience:    sidecar\n\n\
                  Run `tungsten <command> --help` for details on a specific command.\n\
                  Run `tungsten commands` for a flat listing of all commands.\n\
                  Run `tungsten info pipeline` for diagnostic flags and inspection tools."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run a file directly (shorthand for `tungsten run <FILE>`)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Show verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Maximum number of errors to display (0 = no limit)
    #[arg(long, global = true, default_value = "20")]
    pub max_errors: usize,

    /// Dump elaborated type annotations to stderr (diagnostic)
    #[arg(long, global = true)]
    pub dump_types: bool,

    /// Always show diagnostic hints in error output (even in non-TTY contexts)
    #[arg(long, global = true)]
    pub hints: bool,

    /// Suppress diagnostic hints in error output
    #[arg(long, global = true)]
    pub no_hints: bool,

    /// Termination-checking enforcement: `all` | `proofs` | `report`
    /// (ADR 29.6.26e). Overrides `TUNGSTEN_TERMINATION`. Global because the
    /// admission gate runs on `check`, `run`, `test` and `compile` alike.
    #[arg(long, global = true, value_name = "LEVEL")]
    pub termination: Option<String>,
}
