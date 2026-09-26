//! The `info codegen` stand-in for a build without the `codegen` feature.
//!
//! Clap's default answer to `info codegen …` in such a build is
//! `unrecognized subcommand 'codegen'` plus `tip: a similar subcommand exists:
//! 'encoding'` — a suggestion for an unrelated command, and no indication that
//! the cause is a missing build feature. ADR 5.8.26c's retrospective measured
//! the consequence: the documented `info codegen symbols --by-function` route
//! was abandoned mid-investigation in favour of `nm`, which CLAUDE.md warns
//! "will not tell you which kind is which".
//!
//! The message is built by a pure function so it can be asserted without
//! spawning the CLI (the repo's "pure function over injected data" seam).

/// Exit code for "I cannot answer that here" — bad environment, not a finding.
/// Matches the 0 clean / 1 findings / 2 bad-input convention the `doctor`
/// checks use.
const EXIT_BAD_ENVIRONMENT: u8 = 2;

/// The rebuild that makes `info codegen` exist.
pub(crate) const REBUILD_HINT: &str =
    "cargo build -p tungsten_bootstrap --features codegen --bin tungsten";

/// Render the explanation for an `info codegen` invocation that cannot be
/// served, echoing the sub-path the caller asked for so the message names
/// their command rather than a generic one.
///
/// `requested` is the raw trailing argv (`["symbols", "--by-function", "f"]`).
pub(crate) fn unavailable_message(requested: &[String]) -> String {
    let asked = if requested.is_empty() {
        "tungsten info codegen".to_string()
    } else {
        format!("tungsten info codegen {}", requested.join(" "))
    };

    format!(
        "error: `{asked}` needs the `codegen` feature, which this binary was built without.\n\
         \n\
         This is a build-configuration problem, not a typo — the subcommand exists,\n\
         but only in a codegen-featured binary. Rebuild with:\n\
         \n    {REBUILD_HINT}\n\
         \n\
         Note many host `make` targets (check-health, test, the canaries) rebuild\n\
         `target/debug/tungsten` WITHOUT this feature, silently replacing a\n\
         codegen-featured binary — so if this worked a moment ago, a `make` run is\n\
         the likely cause and the command above is the fix.\n\
         \n\
         `make check-codegen` only type-checks the codegen feature; it does not\n\
         produce a usable binary.\n"
    )
}

/// Print the explanation and report the exit status.
///
/// Returns a `u8` rather than an `ExitCode` so the status is *assertable*:
/// `ExitCode` implements neither `PartialEq` nor any accessor, so a test
/// cannot tell `ExitCode::from(2)` from `ExitCode::SUCCESS`. Returning
/// `ExitCode` here left a live mutant — `-> ExitCode` replaced by
/// `Default::default()`, i.e. **SUCCESS**, silently turning "I cannot answer
/// that" into "fine" — which `make mutants-diff` caught. The caller converts
/// at the process edge (ADR 7.7.26f's standing remedy for exit-code mutants).
pub fn cmd_info_codegen_unavailable(requested: &[String]) -> u8 {
    eprint!("{}", unavailable_message(requested));
    EXIT_BAD_ENVIRONMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_exact_subcommand_the_caller_asked_for() {
        let msg = unavailable_message(&[
            "symbols".to_string(),
            "--by-function".to_string(),
            "value_def_list_lookup".to_string(),
        ]);
        assert!(
            msg.contains("tungsten info codegen symbols --by-function value_def_list_lookup"),
            "message should echo the requested command, got:\n{msg}"
        );
    }

    #[test]
    fn bare_invocation_names_the_namespace() {
        let msg = unavailable_message(&[]);
        assert!(msg.contains("`tungsten info codegen` needs"), "got:\n{msg}");
        // No trailing space where the args would have gone.
        assert!(!msg.contains("codegen ` needs"), "got:\n{msg}");
    }

    /// The whole point of the command: a caller must be able to act on it.
    #[test]
    fn carries_the_actionable_rebuild_command() {
        let msg = unavailable_message(&["abi".to_string()]);
        assert!(msg.contains(REBUILD_HINT), "got:\n{msg}");
        assert!(msg.contains("--features codegen"), "got:\n{msg}");
    }

    /// The failure is usually *caused* by a make run, so the message has to say
    /// so — that is the part a reader cannot derive from the error alone.
    #[test]
    fn explains_the_make_target_clobber() {
        let msg = unavailable_message(&[]);
        assert!(msg.contains("make"), "got:\n{msg}");
        assert!(
            msg.contains("WITHOUT this feature"),
            "should name the clobber mechanism, got:\n{msg}"
        );
    }

    /// `make check-codegen` looks like the fix and is not; saying so is the
    /// difference between one round-trip and two.
    #[test]
    fn warns_that_check_codegen_is_not_a_fix() {
        let msg = unavailable_message(&[]);
        assert!(msg.contains("check-codegen"), "got:\n{msg}");
        assert!(msg.contains("does not"), "got:\n{msg}");
    }

    #[test]
    fn exit_code_is_bad_environment_not_success() {
        assert_eq!(EXIT_BAD_ENVIRONMENT, 2);
    }

    /// The whole command's contract: it must report FAILURE. A mutant that
    /// returns `Default::default()` (SUCCESS) turns an unanswerable query into
    /// a silent pass, and this is the assertion that kills it.
    #[test]
    fn dispatch_reports_failure_not_success() {
        let status = cmd_info_codegen_unavailable(&["symbols".to_string()]);
        assert_ne!(status, 0, "must not report success");
        assert_eq!(status, EXIT_BAD_ENVIRONMENT);
    }
}
