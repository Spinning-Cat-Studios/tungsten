//! Rendering, split from the command so the formatting is unit-testable
//! without capturing stdout — including the cases that matter most, where
//! nothing was found and where the walk did not finish.

use std::path::PathBuf;

use super::report::Reachability;

/// Definitions the human report names before it stops counting. The full list
/// is always in `--json`, so the cap bounds the terminal without hiding data.
const NOT_REACHED_SHOWN: usize = 10;

/// One report per root, in the order requested, each byte-identical to what the
/// single-definition form emits — so a reader comparing a before block to an
/// after block across the two spellings is comparing like with like
/// (ADR 3.9.26c AC2).
pub(crate) fn render_human_sections(reports: &[Reachability], file: &PathBuf) -> String {
    reports
        .iter()
        .map(|r| render_human(r, file))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable report for one root.
pub(crate) fn render_human(report: &Reachability, file: &PathBuf) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Externs reachable from `{}` ({}):\n",
        report.root,
        file.display()
    );

    if report.reached.is_empty() {
        // Said explicitly, because "reached nothing" and "examined nothing" are
        // different facts and only one of them is good news.
        let _ = writeln!(
            out,
            "  (none — this definition's call path makes no extern call)"
        );
    } else {
        let width = report
            .reached
            .iter()
            .map(|e| e.symbol.len())
            .max()
            .unwrap_or(0);
        for entry in &report.reached {
            let mark = if entry.executable { "✓" } else { "✗" };
            let _ = writeln!(
                out,
                "  {mark} {:<width$}  via {}",
                entry.symbol,
                entry.via.join(" → "),
                width = width
            );
        }
    }

    let blocking = report.blocking().count();
    let _ = writeln!(
        out,
        "\n  {} definition(s) walked, {} extern(s) reached, {} NOT executable.",
        report.defs_visited,
        report.reached.len(),
        blocking
    );

    if !report.unresolved.is_empty() {
        let _ = writeln!(
            out,
            "  {} global(s) had no definition in this project and were not followed:\n    {}",
            report.unresolved.len(),
            report.unresolved.join(", ")
        );
    }

    if !report.complete() {
        push_incomplete(&mut out, report);
    }

    if blocking > 0 {
        // Sound whether or not the walk finished: an extern that was reached
        // stays reached.
        out.push_str(
            "\n  ✗ A call reaching an unexecutable extern goes silently Stuck: the\n\
             \x20   enclosing assertion never runs and `tungsten test` reports `ok`.\n\
             \x20   Split the decision logic out from the FFI and test that instead.\n",
        );
    } else if report.complete() {
        out.push_str(
            "\n  ✓ Every extern on this call path is executable, so a test of this\n\
             \x20   definition cannot go silently Stuck on one.\n",
        );
        if report.assertable_but_untested() {
            out.push_str(
                "\n  ! ASSERTABLE, AND NOTHING ASSERTS IT: no `test_*` in THIS entry file\n\
                 \x20   calls it directly, so cost-5 coverage it could carry is going unused.\n\
                 \x20   Add an `assert_eq_*` case rather than a type-level one. Per entry\n\
                 \x20   file, like `doctor audit-dead-definitions`: a suite in another\n\
                 \x20   `test_*.tg` is not in this graph, so check there before adding one.\n",
            );
        } else if !report.reached_by_tests.is_empty() {
            let _ = writeln!(
                out,
                "    reached by {} test(s): {}",
                report.reached_by_tests.len(),
                report.reached_by_tests.join(", ")
            );
        }
    }
    out
}

/// The partial-answer verdict: what stopped the walk, and what it did not read.
///
/// Deliberately loud and deliberately placed *above* the clean/blocked verdict,
/// because the failure this guards against is a partial report being believed —
/// which is worse than the silence it replaces (ADR 3.9.26c D2).
fn push_incomplete(out: &mut String, report: &Reachability) {
    use std::fmt::Write;

    let shown: Vec<&str> = report
        .not_reached
        .iter()
        .take(NOT_REACHED_SHOWN)
        .map(String::as_str)
        .collect();
    let more = report.not_reached.len().saturating_sub(shown.len());
    let suffix = if more > 0 {
        format!(", … (+{more} more; --json lists them all)")
    } else {
        String::new()
    };
    let _ = write!(
        out,
        "\n  ⚠ INCOMPLETE: the walk stopped at its --max-visited budget, so this is\n\
         \x20   a PARTIAL answer — any extern reached only through the {} definition(s)\n\
         \x20   below is missing from the list above, and every count is a lower bound.\n\
         \x20   not reached: {}{}\n\
         \x20   Re-run with a larger --max-visited for a complete verdict.\n",
        report.not_reached.len(),
        shown.join(", "),
        suffix
    );
}

/// The `--json` form for a set of roots: an outer array whose every element is
/// exactly what the single form emits.
pub(crate) fn render_json_array(reports: &[Reachability]) -> String {
    format!(
        "[\n{}\n]",
        reports
            .iter()
            .map(render_json)
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

/// The `--json` form, for tooling and acceptance criteria.
///
/// Hand-rolled to match the sibling `info eval externs`, whose registry type
/// lives in `tungsten_core` where a `Serialize` derive would be a presentation
/// concern in the kernel.
///
/// `complete` is an explicit field, not an inference from `not_reached`'s
/// length: a gate branching on `blocking` must be able to see, in one key,
/// that a zero it is about to trust came from a partial walk.
pub(crate) fn render_json(report: &Reachability) -> String {
    let rows: Vec<String> = report
        .reached
        .iter()
        .map(|e| {
            format!(
                "    {{\"symbol\": \"{}\", \"executable\": {}, \"via\": [{}]}}",
                e.symbol,
                e.executable,
                e.via
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let unresolved = quoted(&report.unresolved);
    let not_reached = quoted(&report.not_reached);
    let reached_by_tests = quoted(&report.reached_by_tests);
    format!(
        "{{\n  \"root\": \"{}\",\n  \"defs_visited\": {},\n  \"blocking\": {},\n  \"assertable_but_untested\": {},\n  \"reached_by_tests\": [{}],\n  \"unresolved\": [{}],\n  \"complete\": {},\n  \"not_reached\": [{}],\n  \"reached\": [\n{}\n  ]\n}}",
        report.root,
        report.defs_visited,
        report.blocking().count(),
        report.assertable_but_untested(),
        reached_by_tests,
        unresolved,
        report.complete(),
        not_reached,
        rows.join(",\n")
    )
}

/// A JSON string-array body: `"a", "b"`.
fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("\"{i}\""))
        .collect::<Vec<_>>()
        .join(", ")
}
