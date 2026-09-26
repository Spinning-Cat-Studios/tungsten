//! `tungsten info eval externs` — which `tg_*` externs the evaluator executes.
//!
//! Cost 1: the answer is a compile-time table
//! (`tungsten_core::eval::extern_registry`), so this reads no file and
//! elaborates nothing.
//!
//! It exists because the failure it describes is **silent**. The evaluator
//! executes only the externs it has an arm for; every other `ExternCall` goes
//! `Stuck` with no error and no output, so a `.tg` program calling an
//! unsupported extern appears to run and simply does nothing. ADR 28.7.26a hit
//! exactly that with `println`. Before this command the only way to answer
//! "will the evaluator run this extern?" was to read the dispatch match arms.
//!
//! Pairs with `tungsten doctor check extern-coverage <file>`, which asks the
//! same question of the externs a *specific file* declares.

use std::process::ExitCode;

use tungsten_core::eval::extern_registry::{ExecutableExtern, EXECUTABLE_EXTERNS};

/// Render the executable-extern registry.
pub fn cmd_info_eval_externs(json: bool) -> ExitCode {
    if json {
        println!("{}", render_json(EXECUTABLE_EXTERNS));
    } else {
        print!("{}", render_human(EXECUTABLE_EXTERNS));
    }
    ExitCode::SUCCESS
}

/// The human-readable table.
///
/// Split from the command so the formatting is unit-testable without capturing
/// stdout.
pub(crate) fn render_human(externs: &[ExecutableExtern]) -> String {
    use std::fmt::Write;

    let width = externs.iter().map(|e| e.name.len()).max().unwrap_or(0);
    let mut out = String::new();
    out.push_str("Externs the evaluator executes (all others go silently Stuck):\n\n");
    for entry in externs {
        // `writeln!` into a String is infallible; the Result is discarded for
        // the same reason `push_str` has none.
        let _ = writeln!(
            out,
            "  {:<width$}  [{}]  {}",
            entry.name,
            entry.kind.label(),
            entry.summary,
            width = width
        );
    }
    let _ = writeln!(out, "\n  {} extern(s) registered.", externs.len());
    out.push_str(
        "\n  An extern absent from this list is not an error at elaboration or\n\
         \x20 runtime — the call simply never happens. Check a specific file with:\n\
         \x20   tungsten doctor check extern-coverage <file>\n",
    );
    out
}

/// The `--json` form, for tooling.
///
/// Hand-rolled rather than via `serde`: the registry type lives in
/// `tungsten_core`, and deriving `Serialize` there would put a presentation
/// concern in the kernel for three fields.
pub(crate) fn render_json(externs: &[ExecutableExtern]) -> String {
    let rows: Vec<String> = externs
        .iter()
        .map(|e| {
            format!(
                "    {{\"name\": \"{}\", \"kind\": \"{}\", \"summary\": \"{}\"}}",
                e.name,
                e.kind.label(),
                e.summary.replace('"', "\\\"")
            )
        })
        .collect();
    format!("[\n{}\n]", rows.join(",\n"))
}

// Tests: externs_tests.rs
#[cfg(test)]
#[path = "externs_tests.rs"]
mod tests;
