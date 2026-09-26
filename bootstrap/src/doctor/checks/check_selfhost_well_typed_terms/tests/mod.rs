//! The well-typed-terms verdict, over captured output (ADR 3.9.26h AC2).
//!
//! Every case here is text the self-host actually prints, so the parser is
//! pinned to the producer's format rather than to a guess about it. The three
//! "cannot answer" verdicts get the same weight as the finding ones: they are
//! the reason this check exists, and a parser that quietly turned any of them
//! into a clean census would rebuild the defect it was written to catch.

use super::{
    baseline_for, exit_code_for, outcome_for, parse_census, preflight, Census, IllShapedDefinition,
    MissingInput, Outcome, ProbeVerdict, BASELINE_CORPUS, MAIN_TG_BASELINE,
};
use std::path::Path;

mod rendering;

/// One ill-shaped definition, for building a census of a given size.
fn ill_shaped(count: usize) -> Vec<IllShapedDefinition> {
    (0..count)
        .map(|i| IllShapedDefinition {
            definition: format!("def_{i}"),
            mismatches: vec!["fst over Nat".to_string()],
        })
        .collect()
}

fn examined(count: usize, findings: usize) -> ProbeVerdict {
    ProbeVerdict::Examined(Census {
        examined: count,
        ill_shaped: ill_shaped(findings),
    })
}

/// A clean run over a real corpus.
const CLEAN: &str = "\
✓ src/compiler/main.tg: 2298 definition(s), all OK
[well-typed] census: 2298 definition(s) examined, 0 with shape mismatch(es)";

/// The two shapes ADR 3.9.26h was written for: a curried constructor
/// application, and a tuple projection that ran out of product.
const ILL_SHAPED: &str = "\
[well-typed] build_ctor_app: app over a recursive type
[well-typed] record_field: fst over Nat; snd over a recursive type
[well-typed] census: 3 definition(s) examined, 2 with shape mismatch(es)";

#[test]
fn a_clean_census_is_examined_with_nothing_ill_shaped() {
    assert_eq!(
        parse_census(CLEAN),
        ProbeVerdict::Examined(Census {
            examined: 2298,
            ill_shaped: vec![],
        })
    );
}

#[test]
fn findings_carry_their_definition_and_every_mismatch() {
    let ProbeVerdict::Examined(census) = parse_census(ILL_SHAPED) else {
        panic!("expected a census");
    };
    assert_eq!(census.examined, 3);
    assert_eq!(
        census.ill_shaped,
        vec![
            IllShapedDefinition {
                definition: "build_ctor_app".to_string(),
                mismatches: vec!["app over a recursive type".to_string()],
            },
            IllShapedDefinition {
                definition: "record_field".to_string(),
                mismatches: vec![
                    "fst over Nat".to_string(),
                    "snd over a recursive type".to_string(),
                ],
            },
        ]
    );
}

#[test]
fn a_mismatch_is_split_on_semicolons_not_commas() {
    // A former's description is free to contain a comma — `Adt("T", [A, B])`
    // renders one — and splitting there would turn one finding into two.
    let line = "\
[well-typed] f: case over an ADT, two variants
[well-typed] census: 1 definition(s) examined, 1 with shape mismatch(es)";
    let ProbeVerdict::Examined(census) = parse_census(line) else {
        panic!("expected a census");
    };
    assert_eq!(
        census.ill_shaped[0].mismatches,
        vec!["case over an ADT, two variants".to_string()]
    );
}

#[test]
fn the_census_line_is_not_read_as_a_definition_called_census() {
    // It shares the `[well-typed] ` prefix with a finding and would otherwise
    // parse as one, adding a phantom definition to every clean run.
    let ProbeVerdict::Examined(census) = parse_census(CLEAN) else {
        panic!("expected a census");
    };
    assert!(
        census.ill_shaped.is_empty(),
        "clean run reported findings: {:?}",
        census.ill_shaped
    );
}

#[test]
fn a_free_vars_census_is_not_read_as_a_shape_one() {
    // Both flags can be passed at once and both write to stderr, so the shape
    // parser must ignore the sibling's lines entirely rather than counting them.
    let both = "\
[free-vars] len2: h, t
[free-vars] census: 3 definition(s) examined, 1 with free variable(s)
[well-typed] census: 3 definition(s) examined, 0 with shape mismatch(es)";
    assert_eq!(
        parse_census(both),
        ProbeVerdict::Examined(Census {
            examined: 3,
            ill_shaped: vec![],
        })
    );
}

#[test]
fn a_stubbed_build_is_its_own_verdict_not_a_clean_one() {
    let stubbed = "\
✓ build/probe.tg: 2 definition(s), all OK
[diagnostics] this binary has no diagnostic tools compiled in, so the
              requested flag(s) did nothing.";
    assert_eq!(parse_census(stubbed), ProbeVerdict::Stubbed);
}

#[test]
fn a_stubbed_build_is_recognised_before_the_missing_census() {
    // Both conditions hold for a stubbed build — it prints the marker AND no
    // census. Order matters because the remedies differ: "rebuild the binary
    // differently" versus "the binary predates the flag".
    let stubbed = "[diagnostics] this binary has no diagnostic tools compiled in";
    assert_eq!(parse_census(stubbed), ProbeVerdict::Stubbed);
}

#[test]
fn output_with_no_census_line_at_all_cannot_answer() {
    // The shape a `tungsten1` older than the flag produces: it type-checks the
    // file, ignores the unknown option, and says nothing about shapes.
    let older = "✓ build/probe.tg: 2 definition(s), all OK";
    assert_eq!(parse_census(older), ProbeVerdict::NoCensus);
}

#[test]
fn an_empty_corpus_is_examined_zero_rather_than_no_census() {
    // The caller turns this into a failure, but the *parser* must report it as
    // a census: "examined nothing" and "could not be asked" are different
    // faults and the caller prints different remedies for them.
    let empty = "[well-typed] census: 0 definition(s) examined, 0 with shape mismatch(es)";
    assert_eq!(
        parse_census(empty),
        ProbeVerdict::Examined(Census {
            examined: 0,
            ill_shaped: vec![],
        })
    );
}

#[test]
fn a_malformed_census_count_is_not_silently_zero() {
    // `NoCensus` rather than `Examined(0)`: a count this parser could not read
    // must not become a number the caller reports as fact.
    let malformed = "[well-typed] census: many definition(s) examined, 0 with shape mismatch(es)";
    assert_eq!(parse_census(malformed), ProbeVerdict::NoCensus);
}

#[test]
fn a_finding_with_no_mismatches_is_dropped_rather_than_counted_empty() {
    let odd = "\
[well-typed] weird:
[well-typed] census: 1 definition(s) examined, 1 with shape mismatch(es)";
    let ProbeVerdict::Examined(census) = parse_census(odd) else {
        panic!("expected a census");
    };
    assert!(census.ill_shaped.is_empty());
}

#[test]
fn a_finding_with_an_empty_definition_name_is_dropped() {
    // Guards the `||` in `parse_finding`: with `&&` an entry whose NAME is
    // empty would be reported, and the caller would print `  : fst over Nat`.
    let odd = "\
[well-typed] : fst over Nat
[well-typed] census: 1 definition(s) examined, 1 with shape mismatch(es)";
    let ProbeVerdict::Examined(census) = parse_census(odd) else {
        panic!("expected a census");
    };
    assert!(census.ill_shaped.is_empty(), "{:?}", census.ill_shaped);
}

#[test]
fn a_binary_built_for_another_platform_is_its_own_verdict() {
    // `tungsten1` is a Linux ELF and this check is routinely invoked from the
    // macOS host. Reading that as "no census" tells the reader to rebuild a
    // binary that is already correct.
    let wrong_architecture = "./tungsten1: ./tungsten1: cannot execute binary file";
    assert_eq!(
        parse_census(wrong_architecture),
        ProbeVerdict::NotExecutable
    );
}

#[test]
fn an_exec_format_error_reads_the_same_way() {
    assert_eq!(
        parse_census("bash: ./tungsten1: Exec format error"),
        ProbeVerdict::NotExecutable
    );
}

#[test]
fn a_clean_census_over_an_empty_corpus_fails_rather_than_passing() {
    // AC4's arm. `0 examined, 0 ill-shaped` is textually a clean verdict and
    // semantically no verdict at all — exiting 0 on it would rebuild the silent
    // no-op this whole family of checks was written to catch. Checked BEFORE
    // the baseline, so an empty corpus cannot pass by sitting under one either.
    let verdict = examined(0, 0);
    assert_eq!(outcome_for(&verdict, 0), Outcome::ExaminedNothing);
    assert_eq!(exit_code_for(&outcome_for(&verdict, 0)), 2);
    assert_eq!(outcome_for(&verdict, 300), Outcome::ExaminedNothing);
}

#[test]
fn every_cannot_answer_verdict_exits_two_and_a_finding_exits_one() {
    for unanswerable in [
        ProbeVerdict::Stubbed,
        ProbeVerdict::NotExecutable,
        ProbeVerdict::NoCensus,
    ] {
        assert_eq!(
            exit_code_for(&outcome_for(&unanswerable, 0)),
            2,
            "{unanswerable:?}"
        );
    }
    let found = examined(3, 1);
    assert_eq!(
        exit_code_for(&outcome_for(&found, 0)),
        1,
        "a finding is a different failure from an unanswerable probe"
    );
}

#[test]
fn only_an_examined_corpus_with_nothing_ill_shaped_succeeds() {
    let clean = examined(2298, 0);
    assert_eq!(outcome_for(&clean, 0), Outcome::Clean { examined: 2298 });
    assert_eq!(exit_code_for(&outcome_for(&clean, 0)), 0);
}

// ── The shrink-only baseline (ADR 3.9.26h D4, AC5) ───────────────────────
//
// P0 measured 300 of 2302 on a tree where ADR 3.9.26e was still open, so the
// gate starts as a freeze rather than at 0. The three arms below are the whole
// contract, and the last one is the half that is easy to omit: a gate that only
// fails UPWARD lets a fix be given back for free.

#[test]
fn exactly_the_baseline_passes_and_still_lists_what_it_found() {
    let census = examined(2302, 300);
    let outcome = outcome_for(&census, 300);
    let Outcome::AtBaseline {
        examined: seen,
        ill_shaped: found,
    } = outcome
    else {
        panic!("expected AtBaseline, got {outcome:?}");
    };
    assert_eq!(seen, 2302);
    assert_eq!(found.len(), 300, "the findings are still reported");
    assert_eq!(exit_code_for(&outcome), 0);
}

#[test]
fn one_more_than_the_baseline_is_a_regression() {
    let census = examined(2302, 301);
    let outcome = outcome_for(&census, 300);
    assert!(
        matches!(outcome, Outcome::Regressed { baseline: 300, .. }),
        "{outcome:?}"
    );
    assert_eq!(exit_code_for(&outcome), 1);
}

#[test]
fn fewer_than_the_baseline_fails_until_the_constant_is_lowered() {
    // The shrink-only half. Closing ADR 3.9.26e should take this to a handful,
    // and the gate must say so rather than quietly passing at the old number.
    let census = examined(2302, 35);
    let outcome = outcome_for(&census, 300);
    assert_eq!(
        outcome,
        Outcome::BaselineStale {
            found: 35,
            baseline: 300
        }
    );
    assert_eq!(exit_code_for(&outcome), 1);
}

#[test]
fn zero_findings_under_a_live_baseline_is_stale_rather_than_clean() {
    // `Clean` means "nothing found AND nothing claimed". Reporting it while a
    // baseline of 300 still stands would leave the constant behind forever.
    let census = examined(2302, 0);
    let outcome = outcome_for(&census, 300);
    assert_eq!(
        outcome,
        Outcome::BaselineStale {
            found: 0,
            baseline: 300
        }
    );
}

#[test]
fn the_baseline_applies_to_the_corpus_it_was_measured_on_and_no_other() {
    // A baseline is a property of a corpus. Applying main.tg's baseline to a
    // two-definition fixture would let that fixture hide anything.
    assert_eq!(
        baseline_for(Path::new("src/compiler/main.tg")),
        MAIN_TG_BASELINE
    );
    assert_eq!(
        baseline_for(Path::new("/any/root/src/compiler/main.tg")),
        MAIN_TG_BASELINE,
        "the make target passes an absolute path"
    );
    assert_eq!(baseline_for(Path::new("examples/list.tg")), 0);
    assert_eq!(
        baseline_for(Path::new("src/compiler/elab/main.tg")),
        0,
        "a same-named file elsewhere in the tree is a different corpus"
    );
    assert_eq!(BASELINE_CORPUS, "src/compiler/main.tg");
}

#[test]
fn preflight_names_which_input_is_missing() {
    // AC4's other half: run with no usable inputs and the check says which one,
    // rather than exiting 0. Both absences exit the same way, so an exit-code
    // assertion alone cannot tell a flipped guard from a correct one.
    let existing = std::env::current_exe().expect("the test binary has a path");
    let absent = Path::new("/nonexistent/tungsten1");

    assert_eq!(
        preflight(Path::new("/nonexistent/probe.tg"), &existing),
        Err(MissingInput::SourceFile)
    );
    assert_eq!(
        preflight(&existing, absent),
        Err(MissingInput::SelfhostBinary)
    );
    assert_eq!(
        preflight(Path::new("/nonexistent/probe.tg"), absent),
        Err(MissingInput::SourceFile),
        "the source is checked first"
    );
    assert_eq!(preflight(&existing, &existing), Ok(()));
}
