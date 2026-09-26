//! Every outcome, through the one function that turns it into text and a code.
//!
//! The sibling file asserts what the census *means*; this asserts that saying
//! so out loud does not lose it. `report` and `exit_code_for` are written
//! apart, so an arm that rendered one verdict and returned another's code would
//! satisfy every test there — and the failure mode this whole check exists to
//! catch is precisely a check that exits 0 having learned nothing.

use super::{exit_code_for, ill_shaped, Census, Outcome, ProbeVerdict};
use std::path::Path;
use std::process::ExitCode;

#[test]
fn every_outcome_renders_and_returns_the_code_it_was_assigned() {
    let findings = ill_shaped(2);
    let examined = ProbeVerdict::Examined(Census {
        examined: 1,
        ill_shaped: Vec::new(),
    });
    for outcome in [
        Outcome::CannotAsk(&ProbeVerdict::Stubbed),
        Outcome::CannotAsk(&ProbeVerdict::NotExecutable),
        Outcome::CannotAsk(&ProbeVerdict::NoCensus),
        // An examined census is not a reason the check could not run; the arm
        // exists so the ICE is visible rather than rendered as one of the three.
        Outcome::CannotAsk(&examined),
        Outcome::ExaminedNothing,
        Outcome::Clean { examined: 7 },
        Outcome::AtBaseline {
            examined: 9,
            ill_shaped: &findings,
        },
        Outcome::Regressed {
            examined: 9,
            ill_shaped: &findings,
            baseline: 1,
        },
        Outcome::BaselineStale {
            found: 1,
            baseline: 2,
        },
    ] {
        let assigned = ExitCode::from(exit_code_for(&outcome));
        let rendered = super::super::report(&outcome, Path::new("src/compiler/main.tg"));
        assert_eq!(
            format!("{rendered:?}"),
            format!("{assigned:?}"),
            "{outcome:?}"
        );
    }
}
