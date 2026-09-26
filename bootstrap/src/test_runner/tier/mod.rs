//! Cost tiers for `tungsten test` entry files, read from `tg-test-tiers.toml`
//! (ADR 6.8.26c D4/D7).
//!
//! ## Why the runner reads this, and not the make loop
//!
//! Five `.tg` suites already declared `--check-only` in their own per-file
//! `make` target while `make tg-test`'s blanket glob passed only
//! `--require-tests` — the same file, two modes, one of them written down and
//! ignored. Every test in those files therefore reported `ASSERTED NOTHING`
//! under the gate: correct-by-design cost-3 tests run at cost 5.
//!
//! Fixing that with a second list in `quality.mk` would leave the tier in the
//! per-file target *and* the gate variable, and drift between two copies is
//! silent — a file added to one and not the other produces no error, only a
//! wrong cost tier, which is the defect being fixed. So the tier lives in one
//! data file that the runner resolves, and `tungsten test <file>` honours it
//! with no flag at all.
//!
//! ## The guards
//!
//! Moving the tier somewhere authoritative also makes a *wrong* tier
//! authoritative, and `Skipped` is not a finding — so mis-declaring a file
//! tier 3 would make the vacuity census green by hiding tests rather than
//! fixing them. Two mechanical guards close that (the third, pinning each
//! tier-3 file's `Skipped` count, lives in the ADR's acceptance criteria):
//!
//! - **(a)** a file that calls a runtime assertion may not be declared tier 3.
//! - **(c)** a file matching `must_declare` but absent from `[files]` is an
//!   error, not a default — tier 3 would silently skip every new file, tier 5
//!   would quietly re-open the defect above.
//!
//! Tests: `tier/tests.rs`

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The manifest's filename. The directory holding it is the root every key in
/// it is relative to, so the file's own location defines the project rather
/// than the runner having to guess at one.
pub const MANIFEST_FILENAME: &str = "tg-test-tiers.toml";

/// How much of a test file the runner should actually run.
///
/// Named for what each tier *does* rather than by its number alone: `tier 3` is
/// meaningful only to a reader who already knows the cost scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTier {
    /// Cost 3 — elaborate only. `expect_type`/`expect_error` and typed
    /// let-bindings have already been discharged by the time the body would
    /// run, so there is no runtime assertion to count.
    ElaborateOnly,
    /// Cost 5 — evaluate each body and count the assertions it executes.
    RunBody,
}

impl CostTier {
    /// The cost-scale number this tier is written as in the manifest and in
    /// every ADR that discusses it.
    pub fn number(self) -> u8 {
        match self {
            Self::ElaborateOnly => 3,
            Self::RunBody => 5,
        }
    }

    /// The tier a manifest entry's `tier = N` denotes.
    fn from_number(number: u8) -> Option<Self> {
        match number {
            3 => Some(Self::ElaborateOnly),
            5 => Some(Self::RunBody),
            _ => None,
        }
    }
}

/// Why a tier could not be resolved. Every variant is a hard error rather than
/// a fallback: a defaulted tier is exactly the silent wrong answer this module
/// exists to prevent.
#[derive(Debug, PartialEq, Eq)]
pub enum TierError {
    /// The manifest could not be read or parsed.
    Unreadable { path: PathBuf, detail: String },
    /// A `tier = N` the cost scale does not define.
    UnknownTier { key: String, number: u8 },
    /// Guard (c): the file matches `must_declare` and has no entry.
    Undeclared { key: String },
    /// Guard (a): a tier-3 declaration on a file that asserts at runtime.
    AssertsAtRuntimeButDeclaredTier3 { key: String },
}

impl fmt::Display for TierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => {
                write!(f, "{} is unreadable: {detail}", path.display())
            }
            Self::UnknownTier { key, number } => write!(
                f,
                "{key} declares tier {number}, which is not a cost tier; use 3 \
                 (elaborate only) or 5 (run the body)"
            ),
            Self::Undeclared { key } => write!(
                f,
                "{key} matches `must_declare` in {MANIFEST_FILENAME} but has no \
                 [files] entry. Declare its cost tier: {elaborate} if its \
                 assertions are discharged during elaboration (expect_type, typed \
                 let-bindings), {run} if it asserts at runtime. There is \
                 deliberately no default — tier {elaborate} would silently skip \
                 the file and tier {run} would report every one of its tests as \
                 ASSERTED NOTHING",
                elaborate = CostTier::ElaborateOnly.number(),
                run = CostTier::RunBody.number(),
            ),
            Self::AssertsAtRuntimeButDeclaredTier3 { key } => write!(
                f,
                "{key} is declared tier {elaborate} in {MANIFEST_FILENAME} but \
                 calls a runtime assertion, which tier {elaborate} never executes. \
                 Declaring it tier {elaborate} would report those assertions as \
                 `Skipped` — hiding them rather than running them. Declare it \
                 tier {run}",
                elaborate = CostTier::ElaborateOnly.number(),
                run = CostTier::RunBody.number(),
            ),
        }
    }
}

/// One `[files]` entry.
#[derive(Debug, Deserialize)]
struct FileDeclaration {
    tier: u8,
    /// Why this file is at this tier. Unread by the runner and required of the
    /// author: a bare number is a claim nobody can check.
    #[allow(dead_code)]
    #[serde(default)]
    why: String,
}

/// One `[[expected_failure]]` block — the only sanctioned way to leave a
/// finding un-repaired (D6).
#[derive(Debug, Deserialize)]
struct ExpectedFailureBlock {
    file: String,
    tests: Vec<String>,
    /// The successor ADR that will REMOVE this block. Required by the schema,
    /// not by convention: an expected failure with nobody's name on it is just
    /// a disabled test.
    #[allow(dead_code)]
    adr: String,
    #[allow(dead_code)]
    #[serde(default)]
    why: String,
}

/// The manifest as it sits on disk.
#[derive(Debug, Deserialize)]
struct ManifestFile {
    #[serde(default)]
    must_declare: Vec<String>,
    #[serde(default)]
    files: BTreeMap<String, FileDeclaration>,
    #[serde(default)]
    expected_failure: Vec<ExpectedFailureBlock>,
}

/// A parsed manifest plus the root its keys are relative to.
#[derive(Debug)]
pub struct TierManifest {
    root: PathBuf,
    parsed: ManifestFile,
}

impl TierManifest {
    /// The manifest governing `entry_file`, or `None` if none does.
    ///
    /// Named for what it returns rather than for the search — `find` would say
    /// only that a lookup happens, leaving the caller to guess whether the
    /// answer is a path, a bool, or the manifest itself. Same reasoning as
    /// `registry::entry_for` over `lookup`.
    ///
    /// Walks up from the entry file's directory. `Ok(None)` means no manifest
    /// governs this file — the runner then keeps its historical behaviour
    /// (cost 5 unless `--check-only` is passed), so `tungsten test` still works
    /// outside a project that declares tiers.
    pub fn governing(entry_file: &Path) -> Result<Option<Self>, TierError> {
        let absolute = std::fs::canonicalize(entry_file)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(entry_file));
        let mut dir = absolute.parent();
        while let Some(here) = dir {
            let candidate = here.join(MANIFEST_FILENAME);
            if candidate.is_file() {
                return Self::parse_at(&candidate).map(Some);
            }
            dir = here.parent();
        }
        Ok(None)
    }

    fn parse_at(path: &Path) -> Result<Self, TierError> {
        let text = std::fs::read_to_string(path).map_err(|e| TierError::Unreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        Self::parse(path, &text)
    }

    /// Parse manifest `text` said to live at `path`.
    ///
    /// Split from [`Self::parse_at`] so the filesystem is the only thing that
    /// function does: everything below is driven from a literal TOML string in
    /// tests, which is what makes `expected_failures` and `matches_must_declare`
    /// assertable rather than merely exercised.
    fn parse(path: &Path, text: &str) -> Result<Self, TierError> {
        let parsed: ManifestFile = toml::from_str(text).map_err(|e| TierError::Unreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        Ok(Self {
            root: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            parsed,
        })
    }

    /// `entry_file` as a `/`-separated key relative to the manifest's root, or
    /// `None` when it lies outside that root and so cannot be governed.
    fn key_for(&self, entry_file: &Path) -> Option<String> {
        let absolute = std::fs::canonicalize(entry_file).ok()?;
        let root = std::fs::canonicalize(&self.root).ok()?;
        let relative = absolute.strip_prefix(root).ok()?;
        Some(
            relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    /// The cost tier for `entry_file`, with both guards applied.
    ///
    /// `source` is the entry file's own text, which guard (a) scans.
    pub fn tier_for(&self, entry_file: &Path, source: &str) -> Result<Option<CostTier>, TierError> {
        let Some(key) = self.key_for(entry_file) else {
            return Ok(None);
        };
        decide_tier(
            &key,
            self.parsed.files.get(&key).map(|d| d.tier),
            self.matches_must_declare(&key),
            calls_a_runtime_assertion(source),
        )
    }

    /// The tests in `entry_file` marked expected-failure, each mapped to the
    /// successor ADR that owns removing it.
    ///
    /// Empty when nothing is marked, which is the state this mechanism is
    /// meant to return to.
    pub fn expected_failures(&self, entry_file: &Path) -> BTreeMap<String, String> {
        let Some(key) = self.key_for(entry_file) else {
            return BTreeMap::new();
        };
        self.expected_failures_for_key(&key)
    }

    /// The expected-failure entries for a repo-relative `key`.
    ///
    /// Keyed by the string rather than the path so the lookup is assertable
    /// without a file on disk — `key_for` is the only part that needs one.
    fn expected_failures_for_key(&self, key: &str) -> BTreeMap<String, String> {
        self.parsed
            .expected_failure
            .iter()
            .filter(|block| block.file == key)
            .flat_map(|block| {
                block
                    .tests
                    .iter()
                    .map(|test| (test.clone(), block.adr.clone()))
            })
            .collect()
    }

    fn matches_must_declare(&self, key: &str) -> bool {
        self.parsed
            .must_declare
            .iter()
            .any(|pattern| glob_matches(pattern, key))
    }
}

/// Whether this run should skip test bodies, folding the explicit
/// `--check-only` flag together with the manifest's declaration.
///
/// The flag still wins on its own, so an ad-hoc cost-3 run needs no manifest
/// entry. A tier-3 *declaration* is enough by itself, and that is the whole
/// point: `tungsten test <file>` with no flag must behave under `make tg-test`'s
/// blanket loop exactly as it does under the file's own per-file target.
pub fn should_skip_bodies(flag: bool, declared: Option<CostTier>) -> bool {
    flag || declared == Some(CostTier::ElaborateOnly)
}

/// The decision itself, over the four facts that settle it.
///
/// Pure rather than reaching for the filesystem, because the *precedence* is
/// the part worth asserting: guard (c) fires on a required-but-absent file,
/// guard (a) on a declared-but-asserting one, and an unrequired, undeclared
/// file is simply not governed.
fn decide_tier(
    key: &str,
    declared: Option<u8>,
    must_declare: bool,
    calls_assertion: bool,
) -> Result<Option<CostTier>, TierError> {
    let Some(number) = declared else {
        if must_declare {
            return Err(TierError::Undeclared {
                key: key.to_string(),
            });
        }
        return Ok(None);
    };
    let Some(tier) = CostTier::from_number(number) else {
        return Err(TierError::UnknownTier {
            key: key.to_string(),
            number,
        });
    };
    if tier == CostTier::ElaborateOnly && calls_assertion {
        return Err(TierError::AssertsAtRuntimeButDeclaredTier3 {
            key: key.to_string(),
        });
    }
    Ok(Some(tier))
}

mod scan;
pub use scan::calls_a_runtime_assertion;
use scan::glob_matches;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

// Tests: tests_repo_drift.rs — the checkout-reading drift guards (ADR 31.7.26c)
#[cfg(test)]
#[path = "tests_repo_drift.rs"]
mod tests_repo_drift;
