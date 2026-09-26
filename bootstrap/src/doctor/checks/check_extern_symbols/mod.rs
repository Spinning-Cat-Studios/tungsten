//! `tungsten doctor check link extern-symbols` — does every `extern "C"` a
//! `.tg` module tree declares resolve to a symbol `tungsten_core` exports?
//!
//! Cost 2: two source walks, no elaboration, no LLVM, no container.
//!
//! ## The failure this makes visible
//!
//! An `extern "C"` *declaration* type-checks on every target — the symbol it
//! names is not looked for until something links. So adding a `tg_*` binding to
//! `src/compiler/` and forgetting the Rust side produces a clean `tungsten
//! check`, a clean `cargo test`, a clean `check-health`, and then an
//! `undefined reference` several minutes into a self-compile. ADR 18.8.26b's
//! retrospective named this: the ADR added nine externs and avoided the failure
//! only because `.claude/CLAUDE.md` carries a prose warning to rebuild the
//! container first.
//!
//! ## Not the same question as `extern-coverage`
//!
//! [`super::check_extern_coverage`] asks whether the **evaluator** can execute
//! a declaration — a `run`/`test`/playground question, answered from
//! `EXECUTABLE_EXTERNS`. This asks whether the **linker** can find it — a
//! native-codegen question, answered from `tungsten_core`'s sources. The two
//! answers are independent: 140 of `main.tg`'s externs are unexecutable *and*
//! perfectly linkable.
//!
//! Exit codes follow the `extern-coverage` convention: 0 clean, 2 findings,
//! 1 hard failure (unreadable input).

mod report;
mod scan;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::doctor::checks::check_extern_coverage::{collect_declared_externs, DeclaredExtern};
use crate::driver::modules::parse::parse_module_tree;

use report::{classify, render, verdict_of};
use scan::{exports_in_source, index_by_symbol, ExportedSymbol};

/// Entry point for `tungsten doctor check link extern-symbols <file>`.
pub fn cmd_check_extern_symbols(file: &PathBuf, core_root: &Path, verbose: bool) -> ExitCode {
    let declared = match declared_externs(file) {
        Ok(declared) => declared,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

    // A `--core-root` that does not exist is a hard failure, not an empty scan.
    // Every declaration would otherwise be reported unresolved, which reads as
    // 151 findings about the file and is really one finding about the flag.
    if !core_root.is_dir() {
        eprintln!(
            "error: --core-root {} is not a directory — nothing to scan for exports",
            core_root.display()
        );
        return ExitCode::FAILURE;
    }
    let exports = match scan_exports(core_root) {
        Ok(exports) => exports,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

    let scanned = exports.len();
    let report = classify(&declared, &index_by_symbol(exports));
    print!(
        "{}",
        render(&report, &file.display().to_string(), scanned, verbose)
    );

    // The decision itself is `verdict_of`'s, so it is assertable without
    // spawning this binary; all that is left here is the mapping onto a code.
    if verdict_of(&report, scanned).is_failure() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// Every `extern "C"` declaration in the file's module tree.
fn declared_externs(file: &PathBuf) -> Result<Vec<DeclaredExtern>, String> {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(file, &mut visited, &mut chain, None)
        .map_err(|error| error.to_string())?;
    let mut declared = Vec::new();
    collect_declared_externs(&tree, &mut declared);
    Ok(declared)
}

/// Walk `core_root` for `.rs` files and collect every export they declare.
///
/// The only effectful part of the check; everything it produces flows into
/// pure functions.
fn scan_exports(core_root: &Path) -> Result<Vec<ExportedSymbol>, String> {
    let mut found = Vec::new();
    let mut stack = vec![core_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| format!("reading {}: {error}", dir.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("reading {}: {error}", dir.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("reading {}: {error}", path.display()))?;
                found.extend(exports_in_source(&text, &path.display().to_string()));
            }
        }
    }
    Ok(found)
}
