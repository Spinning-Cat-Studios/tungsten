//! `tungsten doctor check extern-coverage` — which of a file's `extern "C"`
//! declarations the evaluator can actually execute.
//!
//! Cost 2: parse only. Walks the module tree for `extern "C" fn` declarations
//! and cross-references each against
//! `tungsten_core::eval::extern_registry::EXECUTABLE_EXTERNS`.
//!
//! ## The failure this makes visible
//!
//! The evaluator executes only allowlisted externs; every other `ExternCall`
//! goes **silently `Stuck`** — no error, no warning, no output. A program that
//! declares and calls an unlisted extern therefore *appears to run* and does
//! nothing at the call site. ADR 28.7.26a found `println` in exactly that
//! state: it is not one extern but a chain of three, and the whole chain was
//! unreachable on the evaluator, so evaluated programs printed nothing at all.
//!
//! ## Scope, stated plainly
//!
//! An unexecutable extern is **not** a defect in a program destined for native
//! codegen — the symbol links against `libtungsten_core.a` and works. It is a
//! defect only on the LLVM-free evaluator path: `tungsten run`, `tungsten
//! test`, `make tg-test-module`, and the wasm playground. The report says so,
//! because "warning" without "for which target" is how a check gets ignored.
//!
//! Exit codes follow the `nested-patterns` convention: 0 clean, 2 findings,
//! 1 hard failure (unparseable input).

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::eval::extern_registry::{self, EXECUTABLE_EXTERNS};

use crate::ast::Item;
use crate::driver::modules::parse::parse_module_tree;
use crate::driver::modules::ParsedModule;

/// One `extern "C" fn` declaration found in the module tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredExtern {
    /// The C symbol the declaration binds to — `symbol` when given, else the
    /// Tungsten-side name. This is what the evaluator dispatches on, so it is
    /// what must be matched against the registry.
    pub symbol: String,
    /// File the declaration appears in.
    pub file: String,
    /// Byte offset, for `tungsten doctor map-span`.
    pub offset: u32,
}

/// Entry point for `tungsten doctor check extern-coverage <file>`.
pub fn cmd_check_extern_coverage(file: &PathBuf, verbose: bool) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(tree) => tree,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut declared = Vec::new();
    collect_declared_externs(&module_tree, &mut declared);
    let (executable, stuck) = partition_by_executability(&declared);

    print!("{}", render_report(file, &executable, &stuck, verbose));
    if stuck.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

/// Split declarations into those the evaluator executes and those it leaves
/// stuck.
///
/// Split out from the command so the classification is testable without a
/// filesystem or an exit code.
pub(crate) fn partition_by_executability(
    declared: &[DeclaredExtern],
) -> (Vec<DeclaredExtern>, Vec<DeclaredExtern>) {
    declared
        .iter()
        .cloned()
        .partition(|d| extern_registry::is_executable(&d.symbol))
}

/// Render the report.
pub(crate) fn render_report(
    file: &PathBuf,
    executable: &[DeclaredExtern],
    stuck: &[DeclaredExtern],
    verbose: bool,
) -> String {
    let mut out = String::new();
    let total = executable.len() + stuck.len();

    if total == 0 {
        out.push_str(&format!(
            "✓ No `extern \"C\"` declarations found in {}\n",
            file.display()
        ));
        return out;
    }

    if stuck.is_empty() {
        out.push_str(&format!(
            "✓ All {total} declared extern(s) in {} are executable by the evaluator\n",
            file.display()
        ));
    } else {
        out.push_str(&format!(
            "⚠ {} of {total} declared extern(s) are NOT executable by the evaluator:\n\n",
            stuck.len()
        ));
        for entry in stuck {
            out.push_str(&format!(
                "  {}  offset {}  in {}\n",
                entry.symbol, entry.offset, entry.file
            ));
        }
        out.push_str(
            "\nCalls to these go silently Stuck when evaluated — no error, no output,\n\
             the call simply never happens. This affects `tungsten run`, `tungsten test`,\n\
             `make tg-test-module`, and the wasm playground. Native codegen is unaffected:\n\
             the symbols link against libtungsten_core.a and work.\n",
        );
        // A program written for native codegen (the self-hosted compiler is the
        // canonical one — 140 of its 150 externs are filesystem and process
        // FFI) legitimately reports most of its externs here. Saying so keeps a
        // true-but-expected result from reading as a broken check, which is how
        // a diagnostic earns a permanent `|| true`.
        if stuck.len() * 2 > total {
            out.push_str(
                "\nNOTE: most of this file's externs are unsupported, which is normal for a\n\
                 program targeting NATIVE codegen (filesystem, process, and codegen FFI have\n\
                 no evaluator implementation by design). This report matters only if you\n\
                 intend to `run`/`test` this file on the evaluator.\n",
            );
        }
        out.push_str(
            "\nFix (if this file should be evaluable): add a dispatch arm in one of\n\
             the modules `tungsten_core/src/eval/env/handlers/externs/mod.rs`\n\
             tables (`call.rs`, `console.rs`, `arena/`, `builder/`, ...) AND an\n\
             entry in `registry.rs` beside them. See\n\
             docs/repo-memory/adding-tg-ffi-primitives.md step 3.\n\
             \n\
             Use `tungsten doctor map-span <file> <offset>` for file:line:col.\n",
        );
    }

    if verbose && !executable.is_empty() {
        out.push_str("\nExecutable:\n");
        for entry in executable {
            let kind = extern_registry::entry_for(&entry.symbol).map_or("?", |e| e.kind.label());
            out.push_str(&format!(
                "  {}  [{kind}]  in {}\n",
                entry.symbol, entry.file
            ));
        }
    }
    if verbose {
        out.push_str(&format!(
            "\n{} extern(s) registered as executable overall; see `tungsten info eval externs`.\n",
            EXECUTABLE_EXTERNS.len()
        ));
    }
    out
}

/// Recursively collect extern declarations from the parsed module tree.
///
/// `pub(crate)` because `check_extern_symbols` asks a different question of the
/// same declarations — whether the LINKER can find them rather than whether the
/// evaluator can run them. Two walks over one AST would be two things that can
/// disagree about what counts as a declaration.
pub(crate) fn collect_declared_externs(module: &ParsedModule, declared: &mut Vec<DeclaredExtern>) {
    let file_path = module.path.display().to_string();
    for item in &module.source_file.items {
        if let Item::ExternFn(def) = item {
            declared.push(DeclaredExtern {
                // `symbol` overrides the Tungsten-side name when present, and
                // it is the symbol the evaluator dispatches on.
                symbol: def.symbol.clone().unwrap_or_else(|| def.name.name.clone()),
                file: file_path.clone(),
                offset: def.span.start,
            });
        }
    }
    for child in &module.submodules {
        collect_declared_externs(child, declared);
    }
}

// Tests: check_extern_coverage_tests.rs
#[cfg(test)]
#[path = "check_extern_coverage_tests.rs"]
mod tests;
