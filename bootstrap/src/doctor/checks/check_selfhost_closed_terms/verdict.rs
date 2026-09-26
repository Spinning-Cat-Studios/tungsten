//! What the self-host's `--check-free-vars` output means, as a value.
//!
//! Pure: a function from captured text to a verdict, with no subprocess and no
//! filesystem. That is what lets every outcome — including the two the caller
//! can only reach with a specially-built binary — be asserted in a unit test.

/// One definition whose elaborated body is not closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDefinition {
    pub definition: String,
    pub free_variables: Vec<String>,
}

/// What the self-host examined, and what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Census {
    /// How many definitions the self-host looked at.
    pub examined: usize,
    /// The ones that were not closed.
    pub open: Vec<OpenDefinition>,
}

/// The four things the probe can conclude.
///
/// `Stubbed` and `NoCensus` are distinct because the remedies differ — one is
/// "rebuild the binary differently", the other "the binary predates the flag" —
/// and folding them into a single failure would send the reader to the wrong
/// fix half the time.
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
///
/// The printer matches on THIS rather than re-deriving `examined == 0` and
/// `open.is_empty()` from the census: those two guards previously existed in
/// both the printer and the exit-code mapping, so a flipped one was invisible
/// to a test of either. Deciding once removes the duplicate rather than
/// testing it twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// The self-host could not be asked; the variant says why.
    CannotAsk(&'a ProbeVerdict),
    /// It examined nothing, so a clean verdict would be vacuous.
    ExaminedNothing,
    /// Every term is closed.
    Clean { examined: usize },
    /// Some terms are not closed.
    Findings {
        examined: usize,
        open: &'a [OpenDefinition],
    },
}

/// Decide what a verdict means.
#[must_use]
pub fn outcome_for(verdict: &ProbeVerdict) -> Outcome<'_> {
    let ProbeVerdict::Examined(census) = verdict else {
        return Outcome::CannotAsk(verdict);
    };
    if census.examined == 0 {
        return Outcome::ExaminedNothing;
    }
    if census.open.is_empty() {
        return Outcome::Clean {
            examined: census.examined,
        };
    }
    Outcome::Findings {
        examined: census.examined,
        open: &census.open,
    }
}

/// The exit code an outcome produces.
///
/// A plain integer rather than an `ExitCode`: `ExitCode` implements no
/// equality, so the mapping to the number the shell sees would only be
/// assertable by spawning the binary. **A clean census over an empty corpus is
/// a FAILURE here**, which is the whole reason this check exists.
#[must_use]
pub fn exit_code_for(outcome: &Outcome<'_>) -> u8 {
    match outcome {
        Outcome::CannotAsk(_) | Outcome::ExaminedNothing => 2,
        Outcome::Clean { .. } => 0,
        Outcome::Findings { .. } => 1,
    }
}

/// Which inputs the check cannot start without.
///
/// Returned as a value rather than printed inline so the two guards are
/// distinguishable in a test: both produce the same exit code, so a code-only
/// assertion cannot tell a flipped condition from a correct one. The same
/// shape `diff bootstrap-selfhost-check` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingInput {
    SourceFile,
    SelfhostBinary,
}

/// Both inputs must exist before the self-host is spawned.
///
/// # Errors
/// Names whichever input is absent; the source file is checked first.
pub fn preflight(
    file: &std::path::Path,
    selfhost_binary: &std::path::Path,
) -> Result<(), MissingInput> {
    if !file.exists() {
        return Err(MissingInput::SourceFile);
    }
    if !selfhost_binary.exists() {
        return Err(MissingInput::SelfhostBinary);
    }
    Ok(())
}

/// The marker a stubbed build prints when a diagnostic flag is requested.
const STUBBED_MARKER: &str = "[diagnostics] this binary has no diagnostic tools";

/// The census line's prefix.
const CENSUS_PREFIX: &str = "[free-vars] census:";

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

/// A per-definition finding's prefix.
const FINDING_PREFIX: &str = "[free-vars] ";

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
        open: output.lines().filter_map(parse_finding).collect(),
    })
}

/// `[free-vars] census: 2267 definition(s) examined, 0 with free variable(s)`
/// → `Some(2267)`.
fn parse_examined_count(line: &str) -> Option<usize> {
    let rest = line.trim().strip_prefix(CENSUS_PREFIX)?;
    rest.split_whitespace().next()?.parse().ok()
}

/// `[free-vars] len2: h, t` → the definition and its free variables.
///
/// The census line shares the prefix, so it is excluded explicitly rather than
/// by hoping its shape fails to parse — it would otherwise be read as a
/// definition named `census`.
fn parse_finding(line: &str) -> Option<OpenDefinition> {
    let trimmed = line.trim();
    if trimmed.starts_with(CENSUS_PREFIX) {
        return None;
    }
    let rest = trimmed.strip_prefix(FINDING_PREFIX)?;
    let (definition, vars) = rest.split_once(": ")?;
    let free_variables: Vec<String> = vars
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if definition.is_empty() || free_variables.is_empty() {
        return None;
    }
    Some(OpenDefinition {
        definition: definition.to_string(),
        free_variables,
    })
}
