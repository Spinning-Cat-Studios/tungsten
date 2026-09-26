//! How much of the termination report gates a build (ADR 29.6.26e §4, flipped
//! to its target state by ADR 11.8.26b).
//!
//! Every rejection is an error. 29.6.26e shipped `ProofsOnly` as the default
//! because a hard gate would have rejected 557 of the self-hosted compiler's
//! definitions on the day it landed; 11.8.26b's annotation pass drove that
//! residual to zero, so the gate now gates. The knob survives as an escape
//! hatch for bisecting — `--termination proofs` restores the old behaviour, and
//! `report` silences the gate entirely — not as the way it is normally run.

/// Which failures are errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enforcement {
    /// Every failure is an error.
    #[default]
    All,
    /// Only proof-relevant failures are errors — a proof may not reach an
    /// unchecked or partial constant. Executable code is reported, not gated.
    ProofsOnly,
    /// Nothing is an error; the report is informational.
    Report,
}

/// Environment variable selecting the enforcement level.
pub const ENFORCEMENT_VAR: &str = "TUNGSTEN_TERMINATION";

impl Enforcement {
    /// Parse a knob value; an unrecognised one keeps the default rather than
    /// failing a build on a typo in an environment variable.
    ///
    /// Every level is spelled out and the fallback is `default()`, so flipping
    /// which variant is `#[default]` moves the typo case with it. Leaving
    /// `proofs` to be caught by a catch-all would have silently *weakened* the
    /// gate for every misspelling once ADR 11.8.26b made `all` the default.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("all") => Enforcement::All,
            Some("proofs") => Enforcement::ProofsOnly,
            Some("report") => Enforcement::Report,
            _ => Enforcement::default(),
        }
    }

    /// Whether a failure at this proof-relevance is an error.
    #[must_use]
    pub fn gates(self, is_proof_failure: bool) -> bool {
        match self {
            Enforcement::All => true,
            Enforcement::ProofsOnly => is_proof_failure,
            Enforcement::Report => false,
        }
    }
}

/// Process-wide override, set from `--termination` before elaboration starts.
///
/// A global rather than a threaded option because enforcement is a property of
/// the *run*, and the gate sits four call layers below the CLI; `set_max_errors`
/// is the same shape for the same reason.
static OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(NO_OVERRIDE);

/// Sentinel for "no `--termination` was passed".
const NO_OVERRIDE: u8 = u8::MAX;

/// Serializes the tests that manipulate [`OVERRIDE`] (ADR 12.8.26a §5.1).
///
/// It lives beside the resource rather than in either test module because a
/// `Mutex` declared per file does not serialize anything: `#[cfg(test)]` modules
/// across one crate compile into a single multi-threaded test binary, so two
/// per-file locks are two unrelated locks over one global.
///
/// The direction that made this necessary is the reverse of the obvious one.
/// A `ReportingOnly` in flight weakening the gate for another thread is what
/// comes to mind; the one that bites is `set_enforcement(All)` landing while
/// `check_tool_reachability`'s probe is mid-elaboration, which aborts the
/// termination fixture and reports `BlockedByGate` — a false red on the very
/// check ADR 12.8.26a installs, and a gate that flakes is a gate someone
/// deletes.
///
/// Poisoning is recovered rather than propagated: one panicking test should
/// fail alone, not convert every other holder into a second failure.
#[cfg(test)]
pub(crate) static ENFORCEMENT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take [`ENFORCEMENT_LOCK`] for the duration of a test that reads or writes
/// the process-wide enforcement level.
#[cfg(test)]
pub(crate) fn lock_enforcement() -> std::sync::MutexGuard<'static, ()> {
    ENFORCEMENT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Record a `--termination` value for the rest of the run.
pub fn set_enforcement(level: Enforcement) {
    OVERRIDE.store(level.as_code(), std::sync::atomic::Ordering::Relaxed);
}

/// Drop the override, restoring "no `--termination` was passed".
///
/// Only tests need this, and they need it as of ADR 11.8.26b: a test that
/// "restored" the old state by storing `ProofsOnly` used to be a no-op, because
/// that *was* the default. Now it would leave a weakened gate pinned for every
/// later test in the same process.
pub fn reset_enforcement() {
    OVERRIDE.store(NO_OVERRIDE, std::sync::atomic::Ordering::Relaxed);
}

/// Enforcement forced to `Report` for as long as the guard lives, restoring
/// whatever was set before (including "nothing") on drop.
///
/// **Why a report tool needs this (ADR 12.8.26a).** A `doctor check` whose whole
/// job is to print a census has to *reach* its census, and it reaches it by
/// elaborating the file first. Inheriting the build's enforcement level means a
/// file with a rejection aborts elaboration and the tool exits without ever
/// printing the thing the user ran it for — so making the gate stricter silently
/// deletes the diagnostic aimed at exactly the files the gate now rejects. That
/// happened when ADR 11.8.26b flipped the default to `All`.
///
/// The verdict is unaffected: `TerminationTally::exit` counts rejections from
/// the report, not from whether elaboration aborted, so the tool still exits
/// non-zero on a corpus it cannot certify.
#[must_use = "enforcement is restored when the guard drops, so it must be bound"]
pub struct ReportingOnly {
    previous: u8,
}

impl ReportingOnly {
    /// Force `Report` until the returned guard drops.
    pub fn begin() -> Self {
        let previous = OVERRIDE.swap(
            Enforcement::Report.as_code(),
            std::sync::atomic::Ordering::Relaxed,
        );
        ReportingOnly { previous }
    }
}

impl Drop for ReportingOnly {
    fn drop(&mut self) {
        OVERRIDE.store(self.previous, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Enforcement {
    /// Wire encoding for the process-wide override.
    const fn as_code(self) -> u8 {
        match self {
            Enforcement::All => 0,
            Enforcement::ProofsOnly => 1,
            Enforcement::Report => 2,
        }
    }

    /// Inverse of [`Enforcement::as_code`]; `None` for the sentinel.
    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Enforcement::All),
            1 => Some(Enforcement::ProofsOnly),
            2 => Some(Enforcement::Report),
            _ => None,
        }
    }
}

/// The enforcement level for this run: `--termination` if given, else the
/// environment variable, else the default.
#[must_use]
pub fn enforcement() -> Enforcement {
    Enforcement::from_code(OVERRIDE.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or_else(|| Enforcement::parse(std::env::var(ENFORCEMENT_VAR).ok().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_gates_every_rejection() {
        // ADR 11.8.26b: the target state, reached once the corpus annotation
        // pass drove the self-hosted residual to zero.
        let level = Enforcement::default();
        assert_eq!(level, Enforcement::All);
        assert!(level.gates(true));
        assert!(level.gates(false));
    }

    #[test]
    fn proofs_only_still_demotes_executable_rejections() {
        assert!(Enforcement::ProofsOnly.gates(true));
        assert!(!Enforcement::ProofsOnly.gates(false));
    }

    #[test]
    fn all_gates_both_kinds_and_report_gates_neither() {
        assert!(Enforcement::All.gates(true));
        assert!(Enforcement::All.gates(false));
        assert!(!Enforcement::Report.gates(true));
        assert!(!Enforcement::Report.gates(false));
    }

    #[test]
    fn the_knob_parses_its_three_values() {
        assert_eq!(Enforcement::parse(Some("all")), Enforcement::All);
        assert_eq!(Enforcement::parse(Some("report")), Enforcement::Report);
        assert_eq!(Enforcement::parse(Some("proofs")), Enforcement::ProofsOnly);
    }

    #[test]
    fn an_unset_or_misspelled_knob_keeps_the_default() {
        // Not spelled `Enforcement::All` on purpose: the property is "tracks the
        // default", and writing the variant would let a future flip pass while
        // the typo case silently stayed behind.
        for knob in [None, Some("alll"), Some(""), Some("proof")] {
            assert_eq!(Enforcement::parse(knob), Enforcement::default(), "{knob:?}");
        }
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert_eq!(Enforcement::parse(Some("  all  ")), Enforcement::All);
    }

    #[test]
    fn the_override_encoding_round_trips_and_rejects_the_sentinel() {
        for level in [
            Enforcement::All,
            Enforcement::ProofsOnly,
            Enforcement::Report,
        ] {
            assert_eq!(Enforcement::from_code(level.as_code()), Some(level));
        }
        assert_eq!(Enforcement::from_code(NO_OVERRIDE), None);
    }
}
