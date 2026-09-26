//! One-per-run probe of the compiler binary (ADR 21.7.26f / D3).
//!
//! The runner used to fold two very different findings into one silent,
//! exit-0 `(skipped: codegen not available)`:
//!
//! - the binary cannot be executed at all (the devcontainer's Linux ELF sitting
//!   in the bind-mounted `target/release`, ADR 2.7.26b) — a **false green** for
//!   the compile category, and a false *red* on every check/run/error/test test,
//!   each reporting `ERROR: failed to run compiler: …` as if it were a diff;
//! - the binary runs but was built without the `codegen` feature — a legitimate
//!   skip on an LLVM-less host.
//!
//! Both are documented skew classes, and in neither case did the runner name
//! the cause or the remedy. This module separates them and says what to do.

use std::path::Path;
use std::process::Command;

/// What the probe found about the configured compiler binary.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Preflight {
    /// Runnable and carries the `compile` subcommand — every category can run.
    Healthy,
    /// Runnable, but built without the `codegen` feature.
    NoCompileSubcommand,
    /// Cannot be executed on this host; no category can run.
    NotRunnable(String),
}

/// Probe the binary by asking it to list its own commands.
///
/// `tungsten commands` is the cheapest question that distinguishes all three
/// outcomes: it exercises exec, and its output names the subcommands the build
/// actually has.
pub(crate) fn probe(compiler: &Path) -> Preflight {
    let output = match Command::new(compiler).arg("commands").output() {
        Ok(o) => o,
        Err(e) => return Preflight::NotRunnable(e.to_string()),
    };

    // A binary that cannot even list its commands cannot run a category
    // either — treat it like a failed exec rather than a codegen-less build.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().next().unwrap_or("no stderr output");
        return Preflight::NotRunnable(format!("`commands` exited {}: {detail}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.lines().any(|line| line.starts_with("compile ")) {
        Preflight::Healthy
    } else {
        Preflight::NoCompileSubcommand
    }
}

impl Preflight {
    /// Whether the compile category can run.
    pub(crate) fn codegen_available(&self) -> bool {
        matches!(self, Preflight::Healthy)
    }

    /// Whether the whole run must be abandoned.
    pub(crate) fn is_fatal(&self) -> bool {
        matches!(self, Preflight::NotRunnable(_))
    }

    /// The banner printed once before the suite, naming cause and remedy.
    pub(crate) fn banner(&self, compiler: &Path) -> String {
        let header = format!("[golden] preflight: {}", compiler.display());
        let body = match self {
            Preflight::Healthy => return header,
            Preflight::NotRunnable(reason) => format!(
                "[golden] \x1b[31m✗\x1b[0m binary is not runnable on this host ({reason})\n\
                 \x20        → the devcontainer build owns target/release (ADR 2.7.26b bind-mount skew)\n\
                 \x20        → fix: make release"
            ),
            Preflight::NoCompileSubcommand => concat!(
                "[golden] \x1b[33m⊘\x1b[0m `compile` subcommand absent — codegen-less build; compile tests skipped\n",
                "         → expected on LLVM-less hosts; if unexpected (feature clobber), rebuild:\n",
                "           cargo build -p tungsten_bootstrap --features codegen --bin tungsten"
            )
            .to_string(),
        };
        format!("{header}\n{body}")
    }
}

#[cfg(test)]
#[path = "tests_preflight.rs"]
mod tests;
