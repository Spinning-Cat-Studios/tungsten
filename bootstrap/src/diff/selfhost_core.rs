//! `tungsten diff selfhost-core` — do the two compilers elaborate the same
//! definition to the same Core term?
//!
//! Every other differential tool in this namespace compares an *outcome*:
//! `bootstrap-selfhost-check` compares error counts, `exec` compares printed
//! values, `cache` compares cold against warm. None of them can see two
//! compilers agreeing on the verdict while disagreeing about the term — which
//! is precisely the state ADR 19.8.26d found and had to establish by reading
//! the lowering source and constructing a fixture pair to confirm.
//!
//! ## Why a textual comparison is a structural one
//!
//! Both sides render through the same `impl Display for Term` in
//! `tungsten_core`: this binary formats its own `CoreDef`, and the self-host
//! asks `tg_diagnostic_core_term` to format its arena term. So the strings
//! differ exactly when the terms do, and the comparison needs no shared
//! serialisation format.
//!
//! ## What it cannot tell you
//!
//! Agreement on one definition is agreement on one definition. The shape this
//! was built for is systematic — a lowering rule, not a single bad term — so
//! the useful reading is over a definition you have reason to suspect, and the
//! corpus-wide question belongs to
//! `doctor check selfhost closed-terms`.

mod render;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::info::elaborate_for_info;

pub use render::{compare, parse_core_line, preflight};

/// Entry point for `tungsten diff selfhost-core <def> <file>`.
pub fn cmd_diff_selfhost_core(
    definition: &str,
    file: &PathBuf,
    selfhost_binary: &Path,
    verbose: bool,
) -> ExitCode {
    if preflight(selfhost_binary).is_err() {
        eprintln!(
            "error: self-host binary not found: {}",
            selfhost_binary.display()
        );
        eprintln!("  hint: build one with `make devcontainer-self-compile-dev`");
        return ExitCode::FAILURE;
    }

    // --- This compiler's term ---
    let Some(project) = elaborate_for_info(file, verbose, 20) else {
        return ExitCode::FAILURE;
    };
    let Some(def) = project.defs.iter().find(|d| d.name == definition) else {
        eprintln!(
            "error: the bootstrap has no definition `{definition}` in {}",
            file.display()
        );
        eprintln!(
            "  hint: `tungsten info def <name> {}` lists what it does have",
            file.display()
        );
        return ExitCode::FAILURE;
    };
    // `def.term` is a `SpannedTerm`; the inner `Term` is what the self-host
    // renders, so comparing anything else would diff the wrapper.
    let bootstrap_term = format!("{}", def.term.term);

    // --- The self-host's term ---
    let captured = match probe(definition, file, selfhost_binary) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "error: failed to run {}: {error}",
                selfhost_binary.display()
            );
            eprintln!("  hint: ensure `ulimit -s 65536` before running tungsten1");
            return ExitCode::FAILURE;
        }
    };
    if verbose {
        println!(
            "--- {} --dump-core-terms {definition} ---",
            selfhost_binary.display()
        );
        println!("{captured}");
    }

    println!(
        "Comparing Core terms for `{definition}` in {}...\n",
        file.display()
    );
    render::report(&compare(
        &bootstrap_term,
        &parse_core_line(&captured, definition),
    ))
}

/// Ask the self-host for one definition's term, machine-readably.
fn probe(definition: &str, file: &Path, selfhost_binary: &Path) -> std::io::Result<String> {
    let output = Command::new(selfhost_binary)
        .arg("check")
        .arg(file)
        .arg("--dump-core-terms")
        .arg(definition)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}
