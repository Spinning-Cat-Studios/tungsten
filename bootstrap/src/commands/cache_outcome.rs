//! What a `tungsten cache` command produced, as a **value** (ADR 5.8.26d).
//!
//! Every command in this family used to `println!` its way to an `ExitCode`,
//! which is untestable in two directions at once: nothing asserts the text, and
//! `ExitCode` implements neither `PartialEq` nor an accessor, so nothing
//! asserts the code either. Measured on `cache status` written that way: 49.3%
//! diff coverage and 17 surviving mutants, every one an operator inside a
//! format argument. Returning the outcome instead makes both assertable, and
//! `exit()` keeps the mapping in one place rather than repeated per command.

use std::process::ExitCode;

/// The result of a cache command: lines to print, or a reason it could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CacheOutcome {
    /// Lines to write to stdout.
    Reported(Vec<String>),
    /// A message to write to stderr; nothing was inspected or changed.
    Failed(String),
}

impl CacheOutcome {
    /// The process exit code this outcome implies.
    ///
    /// `3` on failure, matching the `cache` family's existing convention for
    /// "could not do the thing" as distinct from "did it, result was empty".
    pub(crate) fn exit(&self) -> ExitCode {
        match self {
            CacheOutcome::Reported(_) => ExitCode::SUCCESS,
            CacheOutcome::Failed(_) => ExitCode::from(3),
        }
    }

    /// Print to the appropriate stream and return the exit code.
    ///
    /// The single place the outcome meets the process, so a command's shell is
    /// three lines and carries no logic worth mutating.
    pub(crate) fn report(&self) -> ExitCode {
        match self {
            CacheOutcome::Reported(lines) => {
                for line in lines {
                    println!("{line}");
                }
            }
            CacheOutcome::Failed(msg) => eprintln!("error: {msg}"),
        }
        self.exit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_outcome_exits_zero_and_a_failed_one_exits_three() {
        // `ExitCode` is not comparable, so compare the debug rendering — the
        // point is that the two differ, which an all-SUCCESS command would not.
        let ok = format!("{:?}", CacheOutcome::Reported(vec![]).exit());
        let bad = format!("{:?}", CacheOutcome::Failed("x".into()).exit());
        assert_ne!(ok, bad, "a failure must not exit like a success");
        assert_eq!(ok, format!("{:?}", ExitCode::SUCCESS));
        assert_eq!(bad, format!("{:?}", ExitCode::from(3)));
    }

    #[test]
    fn report_agrees_with_exit_for_both_variants() {
        // `emit` prints as a side effect; what must not drift is the code it
        // returns versus `exit()`.
        let reported = CacheOutcome::Reported(vec!["a".into()]);
        let failed = CacheOutcome::Failed("boom".into());
        assert_eq!(
            format!("{:?}", reported.report()),
            format!("{:?}", reported.exit())
        );
        assert_eq!(
            format!("{:?}", failed.report()),
            format!("{:?}", failed.exit())
        );
    }
}
