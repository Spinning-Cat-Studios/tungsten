//! Tests: bootstrap/src/doctor/checks/check_tool_reachability/mod.rs
//!
//! The shipped `PAIRINGS` are data that will change as gates change, so the
//! unit tests drive `evaluate`/`render` with their own table. One test does
//! exercise the shipped one — that is the regression guard proper.

use super::{
    evaluate, materialize, render, CompanionPairing, FixtureFile, PairingOutcome, Reachability,
    PAIRINGS,
};
use std::path::Path;

fn always(verdict: Reachability) -> fn(&Path) -> Reachability {
    match verdict {
        Reachability::Reachable => |_: &Path| Reachability::Reachable,
        Reachability::BlockedByGate => |_: &Path| Reachability::BlockedByGate,
    }
}

fn pairing(expected: Reachability, actual: Reachability) -> CompanionPairing {
    CompanionPairing {
        failure_mode: "test failure mode",
        tool: "tungsten doctor check test",
        fixture: &[FixtureFile {
            path: "reachability.tg",
            source: "fn main() -> Nat { 0 }\n",
        }],
        expected,
        probe: always(actual),
    }
}

/// The check fails in **both** directions. The second is the one that matters
/// for the table's honesty: a pairing written off as blocked, then fixed, must
/// stop being written off — otherwise the table decays into a list of excuses
/// nobody revisits.
#[test]
fn drift_is_a_failure_whichever_way_it_went() {
    let regressed = evaluate(&[pairing(
        Reachability::Reachable,
        Reachability::BlockedByGate,
    )]);
    assert!(!regressed[0].agrees(), "a tool that lost its verdict");

    let improved = evaluate(&[pairing(
        Reachability::BlockedByGate,
        Reachability::Reachable,
    )]);
    assert!(!improved[0].agrees(), "a tool that quietly gained one");

    for matched in [Reachability::Reachable, Reachability::BlockedByGate] {
        assert!(
            evaluate(&[pairing(matched, matched)])[0].agrees(),
            "{matched} agreeing with itself"
        );
    }
}

/// The rendering names the tool, the direction of the drift, and the ADR — a
/// reader hitting this in CI has no other context.
#[test]
fn a_drifting_pairing_renders_both_verdicts_and_the_adr() {
    let out = render(&evaluate(&[pairing(
        Reachability::Reachable,
        Reachability::BlockedByGate,
    )]));
    assert!(out.contains("tungsten doctor check test"), "{out}");
    assert!(
        out.contains("expected reachable, got blocked by its gate"),
        "{out}"
    );
    assert!(out.contains("12.8.26a"), "{out}");
    assert!(out.contains("1 of 1"), "{out}");
}

#[test]
fn an_agreeing_table_says_how_many_it_checked() {
    let out = render(&evaluate(&[
        pairing(Reachability::Reachable, Reachability::Reachable),
        pairing(Reachability::BlockedByGate, Reachability::BlockedByGate),
    ]));
    assert!(
        out.contains("✓ 2 companion diagnostic(s) reachable as documented"),
        "{out}"
    );
    assert!(
        !out.contains("expected"),
        "no drift note on a clean run — {out}"
    );
}

/// The regression guard proper: the *shipped* pairings hold.
///
/// This is the assertion that would have failed between ADR 11.8.26b flipping
/// the termination default and 12.8.26a restoring the tool's reachability.
///
/// The only test here that really elaborates — the rest drive stub probes — so
/// it is also the only one exposed to ADR 12.8.26a §5.1: a concurrent
/// `set_enforcement(All)` would abort the termination fixture and report a
/// drift that is not there. The shared lock is what stops this gate flaking.
#[test]
fn the_shipped_pairings_agree_with_their_documentation() {
    let _enforcement = crate::elaborate::termination::lock_enforcement();
    let outcomes = evaluate(PAIRINGS);
    assert!(!outcomes.is_empty(), "the table must not be empty");
    let drifted: Vec<&PairingOutcome> = outcomes.iter().filter(|o| !o.agrees()).collect();
    assert!(drifted.is_empty(), "{}", render(&outcomes));
}

/// The preamble about a gate's own diagnostic appears **only** when some
/// pairing is blocked. Asserting its absence is the half that matters: the
/// mixed-table test above sees it either way, so a condition inverted from
/// `==` to `!=` would still print something and still pass.
#[test]
fn an_all_reachable_table_prints_no_blocked_pairing_preamble() {
    let out = render(&evaluate(&[
        pairing(Reachability::Reachable, Reachability::Reachable),
        pairing(Reachability::Reachable, Reachability::Reachable),
    ]));
    assert!(
        !out.contains("blocked` pairing elaborates"),
        "nothing was blocked, so nothing to explain — {out}"
    );

    let mixed = render(&evaluate(&[pairing(
        Reachability::BlockedByGate,
        Reachability::BlockedByGate,
    )]));
    assert!(
        mixed.contains("blocked` pairing elaborates"),
        "and it does appear when one is — {mixed}"
    );
}

/// The verdict, in both directions. `ExitCode` has neither `PartialEq` nor an
/// accessor, so this enum is the only place the decision is assertable at all.
#[test]
fn the_verdict_follows_agreement_in_both_directions() {
    use super::{verdict, ReachabilityVerdict};

    let agreeing = evaluate(&[pairing(Reachability::Reachable, Reachability::Reachable)]);
    assert_eq!(verdict(&agreeing), ReachabilityVerdict::AllAgree);

    let drifting = evaluate(&[pairing(
        Reachability::Reachable,
        Reachability::BlockedByGate,
    )]);
    assert_eq!(verdict(&drifting), ReachabilityVerdict::Drifted);

    // One bad row among good ones still drifts — `all`, not `any`.
    let mixed = evaluate(&[
        pairing(Reachability::Reachable, Reachability::Reachable),
        pairing(Reachability::Reachable, Reachability::BlockedByGate),
    ]);
    assert_eq!(verdict(&mixed), ReachabilityVerdict::Drifted);
}

/// A pairing's fixture is a file *set*, and the probe is pointed at the first
/// entry — the module-scoped failure modes need siblings to resolve `mod` to
/// (ADR 13.8.26c §2.4).
#[test]
fn a_multi_file_fixture_lands_whole_and_the_entry_file_is_first() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let entry = materialize(
        dir.path(),
        &[
            FixtureFile {
                path: "reachability.tg",
                source: "mod alpha;\n",
            },
            FixtureFile {
                path: "nested/alpha.tg",
                source: "fn one() -> Nat { 1 }\n",
            },
        ],
    );

    assert_eq!(entry, dir.path().join("reachability.tg"), "the first entry");
    assert_eq!(
        std::fs::read_to_string(&entry).expect("entry written"),
        "mod alpha;\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("nested/alpha.tg")).expect("sibling written"),
        "fn one() -> Nat { 1 }\n",
        "a nested path gets its directory created"
    );
}

/// A pairing that names no file is mis-declared, and probing a path that does
/// not exist would report a `BlockedByGate` drift that is not there.
#[test]
#[should_panic(expected = "must name a file")]
fn an_empty_fixture_is_a_mis_declared_pairing() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let _ = materialize(dir.path(), &[]);
}

/// Every shipped row declares a non-empty fixture whose entry file is a `.tg`
/// source. Cheap, and it is the precondition `the_shipped_pairings_agree` has
/// no way to distinguish from a genuine drift.
#[test]
fn every_shipped_pairing_names_an_entry_file() {
    for pairing in PAIRINGS {
        let entry = pairing
            .fixture
            .first()
            .unwrap_or_else(|| panic!("{} declares no fixture", pairing.tool));
        assert!(
            entry.path.ends_with(".tg"),
            "{}'s entry file is {}",
            pairing.tool,
            entry.path
        );
        assert!(
            !pairing.failure_mode.is_empty(),
            "{} names no failure mode",
            pairing.tool
        );
    }
}

/// The parse-only probe demands the **verdict**, not merely the code path.
///
/// A census that ran and found nothing on a fixture built to collide has not
/// reported on the rejected input — which is exactly the failure ADR 13.8.26c
/// §5 warns about: a multimap built over the wrong field reports 0 and looks
/// correct. Without this, the probe's guard could be `true` and its row would
/// still pass, because the shipped fixture does collide.
#[test]
fn the_parse_only_probe_demands_a_finding_not_just_a_parse() {
    use super::pairings::probe_parse_only_census;

    let colliding = tempfile::tempdir().expect("create tempdir");
    materialize(
        colliding.path(),
        &[
            FixtureFile {
                path: "reachability.tg",
                source: "mod alpha;\nmod beta;\nfn main() -> Nat { 0 }\n",
            },
            FixtureFile {
                path: "alpha.tg",
                source: "fn shared() -> Nat { 1 }\n",
            },
            FixtureFile {
                path: "beta.tg",
                source: "fn shared() -> Nat { 2 }\n",
            },
        ],
    );
    assert_eq!(
        probe_parse_only_census(&colliding.path().join("reachability.tg")),
        Reachability::Reachable,
        "a tree with a collision reaches its verdict"
    );

    let clean = tempfile::tempdir().expect("create tempdir");
    materialize(
        clean.path(),
        &[FixtureFile {
            path: "reachability.tg",
            source: "fn main() -> Nat { 0 }\n",
        }],
    );
    assert_eq!(
        probe_parse_only_census(&clean.path().join("reachability.tg")),
        Reachability::BlockedByGate,
        "a tree with nothing to find has produced no verdict about a rejection"
    );
}
