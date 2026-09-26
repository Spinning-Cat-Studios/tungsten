//! The exit decision and the branches only it reaches (ADR 18.8.26b retrospective).
//!
//! Split from the recogniser and classification tests next door because these
//! answer a different question: not "what did the check find?" but "what does
//! the process DO about it?". `ExitCode` implements no equality, so the answer
//! has to be a value before any of it can be asserted — every branch below
//! survived the mutation sweep until `verdict_of` existed to hold it.

use super::super::report::{classify, render, verdict_of, SymbolReport, Verdict};
use super::super::scan::{exports_in_source, index_by_symbol, is_conditional};
use super::super::*;
use super::{declared, exported};

// ---------------------------------------------------------------------------
// The verdict — the exit decision, as a value
// ---------------------------------------------------------------------------

#[test]
fn a_corpus_where_everything_resolves_is_clean() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let report = classify(&[declared("tg_init")], &exports);
    assert_eq!(verdict_of(&report, 176), Verdict::Clean);
    assert!(!verdict_of(&report, 176).is_failure());
}

#[test]
fn an_unresolved_declaration_fails_the_run() {
    let report = classify(&[declared("tg_missing")], &index_by_symbol(vec![]));
    assert_eq!(verdict_of(&report, 176), Verdict::Unresolved);
    assert!(verdict_of(&report, 176).is_failure());
}

/// **Emptiness is checked before findings, and on both sides.** A corpus with
/// no declarations and an export scan that found nothing each make every other
/// answer meaningless — and `0 examined` reported as `Clean` is exactly the
/// green-tick-over-nothing this check exists to catch in other people's code.
#[test]
fn either_side_being_empty_fails_rather_than_reading_clean() {
    let empty_corpus = SymbolReport::default();
    assert_eq!(verdict_of(&empty_corpus, 176), Verdict::ExaminedNothing);

    let no_exports = classify(&[declared("tg_init")], &index_by_symbol(vec![]));
    assert_eq!(
        verdict_of(&no_exports, 0),
        Verdict::ExaminedNothing,
        "an empty export scan must not be reported as 160 findings about the file"
    );

    // Both empty at once is still one verdict, not a panic.
    assert_eq!(
        verdict_of(&SymbolReport::default(), 0),
        Verdict::ExaminedNothing
    );
    assert!(Verdict::ExaminedNothing.is_failure());
}

/// A conditional-only run is **clean**: those symbols link on this target. The
/// distinction matters because folding them into the failure cell would fire
/// the check on a healthy tree.
#[test]
fn a_conditional_only_run_is_clean() {
    let exports = index_by_symbol(vec![exported("tg_wasm", Some("target_arch = \"wasm32\""))]);
    let report = classify(&[declared("tg_wasm")], &exports);
    assert_eq!(verdict_of(&report, 176), Verdict::Clean);
}

// ---------------------------------------------------------------------------
// Rendering branches the verdict does not reach
// ---------------------------------------------------------------------------

#[test]
fn the_conditional_cell_renders_its_gates_and_says_it_does_not_gate() {
    let exports = index_by_symbol(vec![exported("tg_wasm", Some("target_arch = \"wasm32\""))]);
    let rendered = render(
        &classify(&[declared("tg_wasm")], &exports),
        "main.tg",
        176,
        false,
    );
    assert!(rendered.contains("tg_wasm"), "{rendered}");
    assert!(rendered.contains("target_arch"), "{rendered}");
    assert!(rendered.contains("Reported, not gated"), "{rendered}");
}

/// Absent the conditional cell, none of its wording leaks into a clean report —
/// the `is_empty()` guard, which a deleted `!` would invert.
#[test]
fn a_run_with_no_conditional_exports_prints_no_conditional_section() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let rendered = render(
        &classify(&[declared("tg_init")], &exports),
        "main.tg",
        176,
        false,
    );
    assert!(!rendered.contains("Reported, not gated"), "{rendered}");
    assert!(!rendered.contains("gated on"), "{rendered}");
}

/// `--verbose` lists what resolved; without it, nothing. Both halves of the
/// `&&` are asserted, because either alone would let the other flip unnoticed.
#[test]
fn verbose_lists_the_resolved_declarations_and_quiet_does_not() {
    let exports = index_by_symbol(vec![exported("tg_init", None)]);
    let report = classify(&[declared("tg_init")], &exports);

    let quiet = render(&report, "main.tg", 176, false);
    assert!(!quiet.contains("Resolved:"), "{quiet}");

    let loud = render(&report, "main.tg", 176, true);
    assert!(loud.contains("Resolved:"), "{loud}");
    assert!(loud.contains("tg_init"), "{loud}");

    // Verbose with nothing resolved must not print an empty heading.
    let unresolved = classify(&[declared("tg_missing")], &index_by_symbol(vec![]));
    assert!(!render(&unresolved, "main.tg", 176, true).contains("Resolved:"));
}

// ---------------------------------------------------------------------------
// Two narrower recogniser cases the sweep found unasserted
// ---------------------------------------------------------------------------

/// A blank line between a `#[cfg]` and a later export must not carry the gate
/// forward, and must not stop the scan either. The reset arm skips empty lines
/// deliberately, so an attribute run survives the blank lines rustfmt inserts
/// between a doc comment and its item.
#[test]
fn blank_lines_neither_carry_a_gate_forward_nor_end_the_scan() {
    let leaked = "#[cfg(unix)]\nfn something_else() {}\n\n\n#[no_mangle]\n\
                  pub extern \"C\" fn tg_init() {}\n";
    let found = exports_in_source(leaked, "f.rs");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].cfg, None, "a gate leaked across a blank line");

    let kept = "#[cfg(unix)]\n\n#[no_mangle]\n\npub extern \"C\" fn tg_exit(c: i32) -> ! {}\n";
    let found = exports_in_source(kept, "f.rs");
    assert_eq!(found.len(), 1, "blank lines ended the attribute run");
    assert_eq!(found[0].cfg.as_deref(), Some("unix"));
}

/// `negates` must compare the *inner* gate, not merely notice a `not(`. Two
/// gates that are both negations, or a negation of something else, cover
/// nothing between them.
#[test]
fn negates_matches_only_the_gate_it_actually_negates() {
    assert!(!is_conditional(&[
        exported("tg_a", Some("unix")),
        exported("tg_a", Some("not(unix)")),
    ]));
    assert!(
        is_conditional(&[
            exported("tg_b", Some("unix")),
            exported("tg_b", Some("not(windows)")),
        ]),
        "`not(windows)` does not complement `unix`"
    );
    assert!(is_conditional(&[
        exported("tg_c", Some("not(unix)")),
        exported("tg_c", Some("not(windows)")),
    ]));
}

/// `declared_externs` walks a real module tree. Stubbing it to an empty vec
/// would make every run report "nothing examined" — which the verdict correctly
/// calls a failure, but which no test noticed until the sweep asked.
#[test]
fn the_declaration_walk_finds_the_compilers_own_externs() {
    let main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("src/compiler/main.tg");
    let declared = declared_externs(&main).expect("main.tg parses");

    assert!(
        declared.len() > 100,
        "the declaration walk collapsed: {} found",
        declared.len()
    );
    assert!(declared.iter().any(|d| d.symbol == "tg_positivity_check"));
}
