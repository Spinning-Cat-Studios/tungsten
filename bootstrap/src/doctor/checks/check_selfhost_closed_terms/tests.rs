//! The closed-terms verdict, over captured output.
//!
//! Every case here is text the self-host actually prints, so the parser is
//! pinned to the producer's format rather than to a guess about it. The two
//! "cannot answer" verdicts get the same weight as the finding ones: they are
//! the reason this check exists, and a parser that quietly turned either into
//! a clean census would rebuild the defect the check was written to catch.

use super::{
    exit_code_for, outcome_for, parse_census, preflight, Census, MissingInput, OpenDefinition,
    Outcome, ProbeVerdict,
};
use std::path::Path;

/// A clean run over a real corpus.
const CLEAN: &str = "\
✓ src/compiler/main.tg: 2267 definition(s), all OK
[free-vars] census: 2267 definition(s) examined, 0 with free variable(s)";

/// The shape ADR 19.8.26d found: two pattern variables free in one arm.
const OPEN: &str = "\
[free-vars] len2: h, t
[free-vars] drain: t
[free-vars] census: 3 definition(s) examined, 2 with free variable(s)";

#[test]
fn a_clean_census_is_examined_with_nothing_open() {
    assert_eq!(
        parse_census(CLEAN),
        ProbeVerdict::Examined(Census {
            examined: 2267,
            open: vec![],
        })
    );
}

#[test]
fn findings_carry_their_definition_and_every_variable() {
    let ProbeVerdict::Examined(census) = parse_census(OPEN) else {
        panic!("expected a census");
    };
    assert_eq!(census.examined, 3);
    assert_eq!(
        census.open,
        vec![
            OpenDefinition {
                definition: "len2".to_string(),
                free_variables: vec!["h".to_string(), "t".to_string()],
            },
            OpenDefinition {
                definition: "drain".to_string(),
                free_variables: vec!["t".to_string()],
            },
        ]
    );
}

#[test]
fn the_census_line_is_not_read_as_a_definition_called_census() {
    // It shares the `[free-vars] ` prefix with a finding and would otherwise
    // parse as one, adding a phantom definition to every clean run.
    let ProbeVerdict::Examined(census) = parse_census(CLEAN) else {
        panic!("expected a census");
    };
    assert!(
        census.open.is_empty(),
        "clean run reported findings: {:?}",
        census.open
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
    let older = "✓ build/probe.tg: 2 definition(s), all OK";
    assert_eq!(parse_census(older), ProbeVerdict::NoCensus);
}

#[test]
fn an_empty_corpus_is_examined_zero_rather_than_no_census() {
    // The caller turns this into a failure, but the *parser* must report it as
    // a census: "examined nothing" and "could not be asked" are different
    // faults and the caller prints different remedies for them.
    let empty = "[free-vars] census: 0 definition(s) examined, 0 with free variable(s)";
    assert_eq!(
        parse_census(empty),
        ProbeVerdict::Examined(Census {
            examined: 0,
            open: vec![],
        })
    );
}

#[test]
fn a_malformed_census_count_is_not_silently_zero() {
    // `NoCensus` rather than `Examined(0)`: a count this parser could not read
    // must not become a number the caller reports as fact.
    let malformed = "[free-vars] census: many definition(s) examined, 0 with free variable(s)";
    assert_eq!(parse_census(malformed), ProbeVerdict::NoCensus);
}

#[test]
fn a_finding_with_no_variables_is_dropped_rather_than_counted_empty() {
    let odd = "\
[free-vars] weird: 
[free-vars] census: 1 definition(s) examined, 1 with free variable(s)";
    let ProbeVerdict::Examined(census) = parse_census(odd) else {
        panic!("expected a census");
    };
    assert!(census.open.is_empty());
}

#[test]
fn a_binary_built_for_another_platform_is_its_own_verdict() {
    // The real failure the first end-to-end run hit: `tungsten1` is a Linux ELF
    // and the check was invoked from the macOS host. Reading that as "no
    // census" tells the reader to rebuild a binary that is already correct,
    // when the fix is to run inside the devcontainer.
    let wrong_arch = "./tungsten1: ./tungsten1: cannot execute binary file";
    assert_eq!(parse_census(wrong_arch), ProbeVerdict::NotExecutable);
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
    // The arm this whole check exists for. `0 examined, 0 open` is textually a
    // clean verdict and semantically no verdict at all — exiting 0 on it would
    // rebuild the silent no-op the check was written to catch.
    let verdict = ProbeVerdict::Examined(Census {
        examined: 0,
        open: vec![],
    });
    assert_eq!(outcome_for(&verdict), Outcome::ExaminedNothing);
    assert_eq!(exit_code_for(&outcome_for(&verdict)), 2);
}

#[test]
fn every_cannot_answer_verdict_exits_two_and_a_finding_exits_one() {
    for unanswerable in [
        ProbeVerdict::Stubbed,
        ProbeVerdict::NotExecutable,
        ProbeVerdict::NoCensus,
    ] {
        assert_eq!(
            exit_code_for(&outcome_for(&unanswerable)),
            2,
            "{unanswerable:?}"
        );
    }
    let found = ProbeVerdict::Examined(Census {
        examined: 3,
        open: vec![OpenDefinition {
            definition: "len2".to_string(),
            free_variables: vec!["t".to_string()],
        }],
    });
    assert_eq!(
        exit_code_for(&outcome_for(&found)),
        1,
        "a finding is a different failure from an unanswerable probe"
    );
}

#[test]
fn only_an_examined_corpus_with_nothing_open_succeeds() {
    let clean = ProbeVerdict::Examined(Census {
        examined: 2267,
        open: vec![],
    });
    assert_eq!(outcome_for(&clean), Outcome::Clean { examined: 2267 });
    assert_eq!(exit_code_for(&outcome_for(&clean)), 0);
}

#[test]
fn preflight_names_which_input_is_missing() {
    // Both absences exit the same way, so an exit-code assertion cannot tell a
    // flipped guard from a correct one — the same reason
    // `diff bootstrap-selfhost-check` returns this as a value.
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

#[test]
fn a_finding_with_an_empty_definition_name_is_dropped() {
    // Guards the `||` in `parse_finding`: with `&&` an entry whose NAME is
    // empty would be reported, and the caller would print `  : t`.
    let odd = "\
[free-vars] : t
[free-vars] census: 1 definition(s) examined, 1 with free variable(s)";
    let ProbeVerdict::Examined(census) = parse_census(odd) else {
        panic!("expected a census");
    };
    assert!(census.open.is_empty(), "{:?}", census.open);
}
