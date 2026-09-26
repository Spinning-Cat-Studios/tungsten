//! The shared contract every directory-scanning IR audit obeys (ADR 28.7.26e
//! D1/D4): one walker, one candidate/tracked reach measure, one exit code map.
//!
//! Before this module each audit walked the tree itself (`null-calls` and
//! `merge-truncation` had private copies of the walker, `declares` a third
//! outside `ir_checks/` entirely) and each swallowed read errors with
//! `if let Ok(content)`, so an unreadable `.ll` was indistinguishable from a
//! clean one. Both facts are why three of the six audits could be red on the
//! compiler's own emitted IR without anyone noticing.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Bad input — the path is not a directory, or the audit's parser has drifted
/// far enough that it proved nothing. Mirrors `merge-truncation`'s pre-existing
/// rustdoc contract (0 clean / 1 findings / 2 bad input); parser drift IS bad
/// input, so `--strict` vacuity lands here rather than looking like a finding.
const EXIT_BAD_INPUT: u8 = 2;

/// **How far an audit's parser reached**: how many inputs it was *expected* to
/// find something in, and how many it actually parsed.
///
/// The pair travels together because only the pair is meaningful —
/// `tracked == 0` is healthy when `candidates == 0` and a silent failure
/// otherwise (ADR 2.7.26b T5a). Keeping them in one type also means a caller
/// that folds several of these cannot update one counter and forget its partner.
///
/// `tracked` must be a quantity that is **non-zero on healthy IR**: it measures
/// whether the parser still sees anything, not whether it found a defect. An
/// audit that counted *findings* here would fail `--strict` precisely when the
/// compiler is correct.
///
/// (Named for what it measures rather than where it sits: it was `ArmCounts`
/// when only `indirect-buffers` used it, which has two *arms* — but five of the
/// six audits that now share it have one, so "arm" described nothing.)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReachCounts {
    /// Inputs whose shape says the audit SHOULD parse at least one thing.
    pub candidates: usize,
    /// Things it actually parsed.
    pub tracked: usize,
}

impl ReachCounts {
    /// Candidates were found but nothing was parsed — the audit passed without
    /// proving anything, which means its parser has drifted from emitted IR.
    pub fn is_vacuous(&self) -> bool {
        self.candidates > 0 && self.tracked == 0
    }

    /// Add another input's counts into this running total.
    pub fn add(&mut self, other: ReachCounts) {
        self.candidates += other.candidates;
        self.tracked += other.tracked;
    }

    /// Count one input the audit was expected to parse.
    pub fn note_candidate(&mut self) {
        self.candidates += 1;
    }

    /// Count one thing it successfully parsed.
    pub fn note_tracked(&mut self) {
        self.tracked += 1;
    }
}

/// A `.ll` file the audit could not read.
///
/// Unreadable files do not vanish (ADR 28.7.26e D4): they count as candidates
/// nothing could be parsed from, so a corpus the audit cannot open reads as
/// vacuous rather than clean.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CorpusScan {
    /// `.ll` files found under the directory.
    pub files: usize,
    /// Of those, the ones that could not be read.
    pub unreadable: Vec<PathBuf>,
}

impl CorpusScan {
    /// Fold the unreadable files into an audit arm as unparseable candidates.
    pub fn charge_unreadable(&self, counts: &mut ReachCounts) {
        counts.candidates += self.unreadable.len();
    }
}

/// Recursively collect `.ll` files under `dir`, in a deterministic (sorted)
/// order so every audit's report is reproducible.
pub(crate) fn collect_ll_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let before = out.len();
    collect_unsorted(dir, out);
    out[before..].sort();
}

fn collect_unsorted(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_unsorted(&path, out);
        } else if path.extension().is_some_and(|e| e == "ll") {
            out.push(path);
        }
    }
}

/// Walk `dir`'s `.ll` files in sorted order and hand each one's text to
/// `visit`, reporting how many files were seen and which could not be read.
pub(crate) fn scan_ll_corpus(dir: &Path, mut visit: impl FnMut(&Path, &str)) -> CorpusScan {
    let mut files = Vec::new();
    collect_ll_files(dir, &mut files);
    let mut scan = CorpusScan {
        files: files.len(),
        unreadable: Vec::new(),
    };
    for path in &files {
        match std::fs::read_to_string(path) {
            Ok(text) => visit(path, &text),
            Err(_) => scan.unreadable.push(path.clone()),
        }
    }
    scan
}

/// What auditing a directory concluded.
///
/// Separated from the exit code so the decision is unit-testable: `ExitCode`
/// renders opaquely, so a test that asserts only on it asserts very little.
/// [`AuditVerdict::exit`] is the only place a verdict becomes a process status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditVerdict {
    /// The path given was not a directory — nothing was scanned.
    NotADirectory,
    /// The directory held no `.ll` files at all. Unconditionally bad input:
    /// "the audit found nothing wrong with nothing" is the report this whole
    /// ADR exists to stop anyone reading as a pass, and a corpus gate whose
    /// emit silently produced no IR would otherwise go green.
    EmptyCorpus,
    /// The audit found violations.
    Violations(usize),
    /// No violations, but an arm found candidates and parsed nothing, and
    /// `--strict` was requested (ADR 2.7.26b T5a).
    VacuousUnderStrict,
    /// Clean. A vacuous pass without `--strict` lands here too — it warns, but
    /// only `--strict` makes it a failure.
    Clean,
}

impl AuditVerdict {
    /// The verdict for an already-scanned corpus. Violations outrank vacuity:
    /// a corpus with real findings is reported as such whether or not an arm
    /// also proved nothing, so an actionable defect is never masked by a
    /// drift warning.
    pub fn classify(scan: &CorpusScan, vacuous: bool, violations: usize, strict: bool) -> Self {
        if scan.files == 0 {
            return Self::EmptyCorpus;
        }
        if violations > 0 {
            Self::Violations(violations)
        } else if strict && vacuous {
            Self::VacuousUnderStrict
        } else {
            Self::Clean
        }
    }

    /// 0 clean / 1 findings / 2 bad input.
    pub fn exit(&self) -> ExitCode {
        match self {
            Self::Clean => ExitCode::SUCCESS,
            Self::Violations(_) => ExitCode::FAILURE,
            Self::NotADirectory | Self::EmptyCorpus | Self::VacuousUnderStrict => {
                ExitCode::from(EXIT_BAD_INPUT)
            }
        }
    }

    /// The drift / bad-input lines this verdict reports, as a **value**.
    ///
    /// Separated from the printing so the decision is testable: which lines a
    /// verdict produces is real logic (a warning whenever an arm proved nothing,
    /// a failure line only when that actually gates), and a print-only helper's
    /// whole body can be deleted without any test noticing.
    pub fn drift_report(&self, vacuous: bool, what: &str) -> Vec<String> {
        let mut lines = Vec::new();
        if vacuous {
            lines.push(format!(
                "⚠ {what} — parser/format drift? (ADR 2.7.26b T5a, 28.7.26e D4)"
            ));
            // Say so explicitly when the warning did NOT affect the exit code.
            // The default is permissive on purpose — a hand-built fixture
            // directory is often legitimately vacuous (one non-recursive
            // `$direct_mt` forwards no buffers), so strict-by-default would make
            // interactive use noisy. But then a script that forgets `--strict`
            // accepts drift silently, and the only cure is for the warning to
            // name the flag it is not gating on.
            if !matches!(self, Self::VacuousUnderStrict) {
                lines.push(
                    "  (not gating — pass --strict to make this a failure, as \
                     `make check-ir-audits` does)"
                        .to_string(),
                );
            }
        }
        match self {
            Self::VacuousUnderStrict => {
                lines.push("✗ strict mode: vacuous pass is a failure".to_string());
            }
            Self::EmptyCorpus => {
                lines.push("✗ no .ll files found — nothing was audited".to_string());
            }
            _ => {}
        }
        lines
    }

    /// Print [`Self::drift_report`]. Shared so every audit says the same thing.
    pub fn report_vacuity(&self, vacuous: bool, what: &str) {
        for line in self.drift_report(vacuous, what) {
            println!("{line}");
        }
    }
}

/// Report a non-directory path and return the verdict, so each audit's
/// entry point spells the guard once.
pub(crate) fn reject_non_directory(dir: &Path) -> Option<AuditVerdict> {
    if dir.is_dir() {
        return None;
    }
    eprintln!("error: {} is not a directory", dir.display());
    Some(AuditVerdict::NotADirectory)
}

/// Test-only corpus builders, shared by every audit's tests.
///
/// Each audit's tests need a throwaway directory of `.ll` files, and five of
/// them had hand-copied a near-identical `corpus_dir` helper with subtly
/// different signatures (`&[&str]` vs `&[(&str, &str)]`, `tempdir()` vs
/// `TempDir::new()`, `expect` vs `unwrap`). One definition, two shapes.
#[cfg(test)]
pub(crate) mod fixtures {
    /// A corpus of `(relative path, contents)` pairs — use when a test cares
    /// about file names or needs nested directories.
    pub(crate) fn corpus_at(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (relative_path, contents) in files {
            let path = dir.path().join(relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(&path, contents).expect("write corpus file");
        }
        dir
    }

    /// A corpus of IR modules named `m0.ll`, `m1.ll`, … — use when only the
    /// contents matter, which is the common case.
    pub(crate) fn corpus_of(modules: &[&str]) -> tempfile::TempDir {
        let named: Vec<(String, &str)> = modules
            .iter()
            .enumerate()
            .map(|(i, module)| (format!("m{i}.ll"), *module))
            .collect();
        let pairs: Vec<(&str, &str)> = named
            .iter()
            .map(|(name, module)| (name.as_str(), *module))
            .collect();
        corpus_at(&pairs)
    }
}

#[cfg(test)]
mod tests;
