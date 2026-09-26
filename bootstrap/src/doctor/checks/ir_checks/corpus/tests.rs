//! Tests for the shared corpus contract (ADR 28.7.26e D1/D4): the walker's
//! determinism and I/O honesty, the reach measure, and the exit-code map.

use super::fixtures::corpus_at as corpus;
use super::*;

#[test]
fn walker_recurses_and_returns_a_deterministic_order() {
    let dir = corpus(&[
        ("z.ll", ""),
        ("a.ll", ""),
        ("nested/m.ll", ""),
        ("nested/deep/b.ll", ""),
        ("ignored.txt", "not IR"),
    ]);
    let mut files = Vec::new();
    collect_ll_files(dir.path(), &mut files);

    let names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 4, "only .ll files: {names:?}");
    assert!(!names.contains(&"ignored.txt".to_string()));

    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "report order must be reproducible");
}

#[test]
fn walker_appends_rather_than_replacing() {
    let dir = corpus(&[("a.ll", "")]);
    let mut files = vec![std::path::PathBuf::from("/pre-existing.ll")];
    collect_ll_files(dir.path(), &mut files);
    assert_eq!(files.len(), 2, "the caller's entries survive: {files:?}");
    assert_eq!(files[0], std::path::PathBuf::from("/pre-existing.ll"));
}

#[test]
fn missing_directory_yields_no_files() {
    let mut files = Vec::new();
    collect_ll_files(std::path::Path::new("/nonexistent/28726e"), &mut files);
    assert!(files.is_empty());
}

#[test]
fn scan_visits_every_readable_file_with_its_text() {
    let dir = corpus(&[("a.ll", "alpha"), ("nested/b.ll", "beta")]);
    let mut seen: Vec<String> = Vec::new();
    let scan = scan_ll_corpus(dir.path(), |_, text| seen.push(text.to_string()));
    seen.sort();
    assert_eq!(seen, vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(scan.files, 2);
    assert!(scan.unreadable.is_empty());
}

/// An unreadable `.ll` must not read as a clean one (ADR 28.7.26e D4). A
/// directory named `x.ll` is the portable stand-in for "cannot be read as
/// text": `read_to_string` fails on it without needing chmod semantics.
#[test]
fn unreadable_files_are_counted_not_skipped() {
    let dir = corpus(&[("good.ll", "define void @f() {\n}\n")]);
    std::fs::create_dir(dir.path().join("bad.ll")).expect("mkdir bad.ll");

    let mut visited = 0usize;
    let scan = scan_ll_corpus(dir.path(), |_, _| visited += 1);
    assert_eq!(visited, 1, "only the readable file reaches the audit");
    assert_eq!(scan.files, 1, "a directory is not a .ll file");

    // The same shape via the constructor the audits actually charge from.
    let scan = CorpusScan {
        files: 3,
        unreadable: vec!["a.ll".into(), "b.ll".into()],
    };
    let mut counts = ReachCounts::default();
    scan.charge_unreadable(&mut counts);
    assert_eq!(
        counts,
        ReachCounts {
            candidates: 2,
            tracked: 0
        },
        "unreadable files are candidates nothing could be parsed from"
    );
    assert!(
        counts.is_vacuous(),
        "so the corpus reads as drifted, not clean"
    );
}

#[test]
fn vacuity_needs_candidates_without_tracked() {
    let mut counts = ReachCounts::default();
    assert!(!counts.is_vacuous(), "an empty corpus proves nothing wrong");

    counts.note_candidate();
    assert!(counts.is_vacuous(), "a candidate nothing was parsed from");

    counts.note_tracked();
    assert!(!counts.is_vacuous(), "parsed something → non-vacuous");
    assert_eq!(
        counts,
        ReachCounts {
            candidates: 1,
            tracked: 1
        }
    );
}

#[test]
fn add_sums_both_fields() {
    let mut total = ReachCounts {
        candidates: 2,
        tracked: 5,
    };
    total.add(ReachCounts {
        candidates: 3,
        tracked: 7,
    });
    assert_eq!(
        total,
        ReachCounts {
            candidates: 5,
            tracked: 12
        },
        "a fold that updates one field and forgets its partner fails here"
    );
}

#[test]
fn violations_outrank_vacuity_in_every_strictness_setting() {
    let scanned = CorpusScan {
        files: 7,
        unreadable: Vec::new(),
    };
    for strict in [false, true] {
        assert_eq!(
            AuditVerdict::classify(&scanned, true, 3, strict),
            AuditVerdict::Violations(3),
            "strict={strict}: an actionable defect is never masked by drift"
        );
    }
    assert_eq!(
        AuditVerdict::classify(&scanned, true, 0, false),
        AuditVerdict::Clean,
        "vacuity warns without --strict"
    );
    assert_eq!(
        AuditVerdict::classify(&scanned, true, 0, true),
        AuditVerdict::VacuousUnderStrict
    );
    assert_eq!(
        AuditVerdict::classify(&scanned, false, 0, true),
        AuditVerdict::Clean
    );
}

/// A corpus with no IR in it outranks everything: "found nothing wrong with
/// nothing" is the report this ADR exists to stop reading as a pass, so it is
/// bad input in every strictness setting.
#[test]
fn an_empty_corpus_outranks_every_other_verdict() {
    let empty = CorpusScan::default();
    for (vacuous, violations, strict) in [
        (false, 0, false),
        (true, 0, true),
        (false, 3, false),
        (true, 3, true),
    ] {
        assert_eq!(
            AuditVerdict::classify(&empty, vacuous, violations, strict),
            AuditVerdict::EmptyCorpus,
            "vacuous={vacuous} violations={violations} strict={strict}"
        );
    }
}

/// 0 clean / 1 findings / 2 bad input — the contract CI reads.
#[test]
fn exit_codes_distinguish_clean_findings_and_bad_input() {
    let render = |v: AuditVerdict| format!("{:?}", v.exit());
    let clean = render(AuditVerdict::Clean);
    let findings = render(AuditVerdict::Violations(1));
    let bad_input = render(AuditVerdict::NotADirectory);

    assert_eq!(clean, format!("{:?}", ExitCode::SUCCESS));
    assert_eq!(findings, format!("{:?}", ExitCode::FAILURE));
    assert_eq!(bad_input, format!("{:?}", ExitCode::from(EXIT_BAD_INPUT)));
    assert_eq!(
        render(AuditVerdict::VacuousUnderStrict),
        bad_input,
        "parser drift is bad input, not a finding"
    );
    assert_eq!(
        render(AuditVerdict::EmptyCorpus),
        bad_input,
        "an empty corpus is bad input"
    );
    for pair in [
        (&clean, &findings),
        (&clean, &bad_input),
        (&findings, &bad_input),
    ] {
        assert_ne!(pair.0, pair.1, "the three outcomes must be distinguishable");
    }
}

#[test]
fn non_directory_paths_are_rejected_before_scanning() {
    let dir = corpus(&[("a.ll", "")]);
    assert_eq!(reject_non_directory(dir.path()), None, "a directory passes");
    assert_eq!(
        reject_non_directory(&dir.path().join("a.ll")),
        Some(AuditVerdict::NotADirectory),
        "a plain file is not a corpus"
    );
    assert_eq!(
        reject_non_directory(&dir.path().join("missing")),
        Some(AuditVerdict::NotADirectory)
    );
}

/// The drift report as a value: a warning whenever an arm proved nothing, and a
/// failure line only from the verdicts that actually gate — the two halves of
/// the T5a contract.
#[test]
fn drift_report_warns_on_vacuity_and_fails_only_where_it_gates() {
    let warning = "⚠ an arm parsed nothing — parser/format drift? \
                   (ADR 2.7.26b T5a, 28.7.26e D4)"
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let report = |verdict: AuditVerdict, vacuous: bool| {
        verdict
            .drift_report(vacuous, "an arm parsed nothing")
            .join("\n")
    };

    // Vacuity alone warns, whatever the verdict, and says what drifted — plus a
    // second line naming the flag it is NOT gating on, so a permissive run
    // cannot look like a clean one to someone reading only the output.
    for verdict in [AuditVerdict::Clean, AuditVerdict::Violations(1)] {
        let lines = verdict.drift_report(true, "an arm parsed nothing");
        assert_eq!(lines.len(), 2, "{verdict:?}: {lines:?}");
        assert_eq!(lines[0], warning);
        assert!(lines[1].contains("--strict"), "{:?}", lines[1]);
        assert!(lines[1].contains("not gating"), "{:?}", lines[1]);
    }

    // Non-vacuous, non-gating verdicts say nothing at all.
    for verdict in [AuditVerdict::Clean, AuditVerdict::Violations(1)] {
        assert_eq!(report(verdict.clone(), false), "", "{verdict:?}");
    }
    assert_eq!(
        report(AuditVerdict::NotADirectory, false),
        "",
        "the non-directory error is already reported to stderr"
    );

    // The two gating verdicts add their own failure line, each distinct.
    let vacuous_strict = report(AuditVerdict::VacuousUnderStrict, false);
    let empty = report(AuditVerdict::EmptyCorpus, false);
    assert!(vacuous_strict.contains("strict mode"), "{vacuous_strict}");
    assert!(empty.contains("no .ll files found"), "{empty}");
    assert_ne!(vacuous_strict, empty, "distinct causes, distinct messages");

    // A vacuous corpus under --strict reports both: what drifted, then that it
    // gated — the warning must not be swallowed by the failure line.
    let both = AuditVerdict::VacuousUnderStrict.drift_report(true, "an arm parsed nothing");
    assert_eq!(both.len(), 2, "{both:?}");
    assert_eq!(both[0], warning);
    assert!(both[1].contains("strict mode"));
}

#[test]
fn report_vacuity_is_callable_for_every_verdict() {
    for verdict in [
        AuditVerdict::Clean,
        AuditVerdict::Violations(1),
        AuditVerdict::VacuousUnderStrict,
        AuditVerdict::NotADirectory,
        AuditVerdict::EmptyCorpus,
    ] {
        verdict.report_vacuity(true, "an arm parsed nothing");
        verdict.report_vacuity(false, "an arm parsed nothing");
    }
}
