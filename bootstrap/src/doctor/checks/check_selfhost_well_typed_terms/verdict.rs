//! What the self-host's `--check-well-typed` output means, as a value.
//!
//! Pure: a function from captured text to a verdict, with no subprocess and no
//! filesystem — the same split `check_selfhost_closed_terms` uses, and for the
//! same reason. Every outcome, including the three the caller can only reach
//! with a specially-built (or absent) binary, is then assertable without a
//! self-compile.

/// One definition in which an eliminator stands over the wrong former.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllShapedDefinition {
    pub definition: String,
    /// Each reads `<eliminator> over <former>`, e.g. `fst over Nat`.
    pub mismatches: Vec<String>,
}

/// What the self-host examined, and what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// How many definitions the self-host looked at.
    pub examined: usize,
    /// The ones with at least one mismatch.
    pub ill_shaped: Vec<IllShapedDefinition>,
}

/// The four things the probe can conclude.
///
/// The same four as the closed-terms check, and deliberately not shared with
/// it: the two carry different census prefixes, and a parser generic over both
/// would have to be told which one it was reading, which is the parameter that
/// gets passed wrongly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// The binary has no diagnostic tools compiled in.
    Stubbed,
    /// The binary could not be executed at all — most often a Linux `tungsten1`
    /// invoked from the macOS host rather than inside the devcontainer.
    NotExecutable,
    /// The binary ran but printed no census line.
    NoCensus,
    /// The binary reported a census.
    Examined(Census),
}

/// What the check concluded, once and for all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// The self-host could not be asked; the variant says why.
    CannotAsk(&'a ProbeVerdict),
    /// It examined nothing, so a clean verdict would be vacuous.
    ExaminedNothing,
    /// Every eliminator stands over a former that admits it, and no baseline
    /// claims otherwise.
    Clean { examined: usize },
    /// Exactly the recorded baseline: the shrink-only contract holds.
    AtBaseline {
        examined: usize,
        ill_shaped: &'a [IllShapedDefinition],
    },
    /// More than the baseline allows.
    Regressed {
        examined: usize,
        ill_shaped: &'a [IllShapedDefinition],
        baseline: usize,
    },
    /// Fewer than the baseline claims — it must be lowered, so the gain cannot
    /// be silently given back later. The same contract `check-adr-links` and
    /// `tools/selfhost-conformance` use.
    BaselineStale { found: usize, baseline: usize },
}

/// Decide what a verdict means against a corpus's baseline.
#[must_use]
pub fn outcome_for(verdict: &ProbeVerdict, baseline: usize) -> Outcome<'_> {
    let ProbeVerdict::Examined(census) = verdict else {
        return Outcome::CannotAsk(verdict);
    };
    if census.examined == 0 {
        return Outcome::ExaminedNothing;
    }
    let found = census.ill_shaped.len();
    if found > baseline {
        return Outcome::Regressed {
            examined: census.examined,
            ill_shaped: &census.ill_shaped,
            baseline,
        };
    }
    if found < baseline {
        return Outcome::BaselineStale { found, baseline };
    }
    if baseline == 0 {
        return Outcome::Clean {
            examined: census.examined,
        };
    }
    Outcome::AtBaseline {
        examined: census.examined,
        ill_shaped: &census.ill_shaped,
    }
}

/// The exit code an outcome produces.
///
/// A plain integer rather than an `ExitCode`, which implements no equality and
/// would only be assertable by spawning the binary. **A clean census over an
/// empty corpus is a FAILURE**: `0 examined` and `0 findings` must not render
/// alike, which is the failure mode ADR 19.8.26d's retrospective actually hit.
#[must_use]
pub fn exit_code_for(outcome: &Outcome<'_>) -> u8 {
    match outcome {
        Outcome::CannotAsk(_) | Outcome::ExaminedNothing => 2,
        Outcome::Clean { .. } | Outcome::AtBaseline { .. } => 0,
        Outcome::Regressed { .. } | Outcome::BaselineStale { .. } => 1,
    }
}

/// The definitions `src/compiler/main.tg` carries today (ADR 3.9.26h P0).
///
/// **Measured before either motivating defect was fixed** (D3), on a tree where
/// ADR 3.9.26e was still open: 300 of 2302 definitions, 552 occurrences.
/// 517 of those are `app` over a sum, a μ or a product — 3.9.26e's shape
/// exactly, a constructor's curried arrow paired with a UNARY lambda, confirmed
/// by dumping `errors_push`. The remaining 35 are `fst`/`snd` over something no
/// recorded type says is a pair — a projection that overshot the end of its
/// product, adjacent to ADR 4.9.26b but *mostly* not the same fault: 4.9.26b's
/// extra `fst` is over a genuine product, which this check cannot see.
///
/// **Two of the 35 WERE 4.9.26b's**, which is why the constant is 298 and not
/// 300 (ADR 4.9.26b AC6). The overshoot is invisible only while the field it
/// lands in is itself a pair; where `main.tg` reads the last field of a record
/// whose tail is a two-element tuple of scalars, the extra `fst` lands on a
/// scalar and this check does see it. The parenthetical above read the fault
/// correctly and its *reach* too narrowly — a reminder that "invisible to check
/// X" is a claim about a corpus, not about a defect.
///
/// So this gate is **shrink-only** rather than at 0 (D4): the number may only
/// go down, and a run that finds fewer FAILS until the constant is lowered, so
/// a fix cannot be silently given back. Closing both classes takes it to 0, at
/// which point `Clean` becomes reachable and this constant retires.
///
/// **Re-measured by ADR 18.9.26e: 303.** Its check found the constant already
/// stale — the tree it started from measured **292**, six below 298, and nothing
/// had lowered it. Its integer-match mirror then added **11** definitions, every
/// one `app` over a sum or a μ: 3.9.26e's shape, which ANY self-host function
/// that builds a multi-field constructor (`ElabExprOk(…)`, `CIRIf(…)`,
/// `PatLiteral(…)`) adds until 3.9.26e lands. So the number rose for new code
/// of the recorded class, not for a new class; `app` over anything else, or a
/// `fst`/`snd` finding, in a later diff is still a regression.
///
/// **In code rather than in an environment variable**, for the reason
/// `tools/selfhost-conformance`'s `TERMINATION_BASELINE` is: a number no file
/// commits is a number nobody can regress against.
pub const MAIN_TG_BASELINE: usize = 303;

/// The corpus [`MAIN_TG_BASELINE`] was measured on.
pub const BASELINE_CORPUS: &str = "src/compiler/main.tg";

/// The baseline that applies to `file`.
///
/// A baseline is a property of a **corpus**, not of the check: applying
/// main.tg's 300 to a two-definition fixture would let that fixture hide
/// anything, and a gate that passes over an unmeasured corpus is the silent
/// no-op this family of checks exists to refuse.
#[must_use]
pub fn baseline_for(file: &std::path::Path) -> usize {
    if file.ends_with(BASELINE_CORPUS) {
        MAIN_TG_BASELINE
    } else {
        0
    }
}

/// The marker a stubbed build prints when a diagnostic flag is requested.
const STUBBED_MARKER: &str = "[diagnostics] this binary has no diagnostic tools";

/// The census line's prefix.
const CENSUS_PREFIX: &str = "[well-typed] census:";

/// A per-definition finding's prefix.
const FINDING_PREFIX: &str = "[well-typed] ";

/// The separator between one definition's findings.
///
/// A semicolon, not a comma: a mismatch reads `fst over a product`, and a
/// former's description is free to contain a comma later without silently
/// splitting one finding into two.
const MISMATCH_SEPARATOR: char = ';';

/// What a shell says when asked to run a binary built for another platform.
///
/// Checked because the alternative reading — "no census, so the binary predates
/// the flag" — sends the reader to rebuild a binary that is already correct,
/// when the actual fix is to run the check inside the devcontainer.
const NOT_EXECUTABLE_MARKERS: &[&str] = &[
    "cannot execute binary file",
    "Exec format error",
    "Permission denied",
];

/// Read a verdict out of the self-host's captured output.
///
/// The stubbed check comes first because a stubbed build prints no census at
/// all: reading them in the other order would report `NoCensus` and send the
/// reader looking for a missing flag rather than a missing build.
#[must_use]
pub fn parse_census(output: &str) -> ProbeVerdict {
    if output.contains(STUBBED_MARKER) {
        return ProbeVerdict::Stubbed;
    }
    if NOT_EXECUTABLE_MARKERS
        .iter()
        .any(|marker| output.contains(marker))
    {
        return ProbeVerdict::NotExecutable;
    }
    let Some(examined) = output.lines().find_map(parse_examined_count) else {
        return ProbeVerdict::NoCensus;
    };
    ProbeVerdict::Examined(Census {
        examined,
        ill_shaped: output.lines().filter_map(parse_finding).collect(),
    })
}

/// `[well-typed] census: 2298 definition(s) examined, 0 with shape mismatch(es)`
/// → `Some(2298)`.
fn parse_examined_count(line: &str) -> Option<usize> {
    let rest = line.trim().strip_prefix(CENSUS_PREFIX)?;
    rest.split_whitespace().next()?.parse().ok()
}

/// `[well-typed] build_row: fst over Nat; app over a recursive type` → the
/// definition and each mismatch.
///
/// The census line shares the prefix, so it is excluded explicitly rather than
/// by hoping its shape fails to parse — it would otherwise be read as a
/// definition named `census`.
fn parse_finding(line: &str) -> Option<IllShapedDefinition> {
    let trimmed = line.trim();
    if trimmed.starts_with(CENSUS_PREFIX) {
        return None;
    }
    let rest = trimmed.strip_prefix(FINDING_PREFIX)?;
    let (definition, findings) = rest.split_once(": ")?;
    let mismatches: Vec<String> = findings
        .split(MISMATCH_SEPARATOR)
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    if definition.is_empty() || mismatches.is_empty() {
        return None;
    }
    Some(IllShapedDefinition {
        definition: definition.to_string(),
        mismatches,
    })
}
