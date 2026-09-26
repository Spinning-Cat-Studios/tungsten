//! Turning a search that found no satisfying assignment into rejections
//! (ADR 29.6.26e §2.3): which member to blame, and for what.

use std::collections::BTreeMap;

use super::Signature;
use crate::terms::termination::report::{FailureReason, TerminationFailure};
use crate::terms::termination::size_env::CallSite;
use crate::terms::termination::DefView;

/// Rejections for a group where no assignment works: name, per member, the
/// first call that fails under that member's first candidate root.
pub(super) fn descent_failures(
    group: &[String],
    signatures: &[Signature<'_>],
    candidates: &[Vec<usize>],
    sites: &[BTreeMap<usize, Vec<CallSite>>],
) -> Vec<TerminationFailure> {
    let probe: Vec<usize> = candidates.iter().map(|positions| positions[0]).collect();
    group
        .iter()
        .enumerate()
        .filter_map(|(member, name)| {
            let root = probe[member];
            let failing = sites[member][&root].iter().find(|site| {
                group
                    .iter()
                    .position(|other| *other == site.callee)
                    .is_some_and(|callee| {
                        let position = probe[callee];
                        !(site.supplies(position) && site.descends_at(position))
                    })
            })?;
            let callee = group
                .iter()
                .position(|other| *other == failing.callee)
                .expect("a failing site names a group member");
            let position = probe[callee];
            let reason = if failing.supplies(position) {
                FailureReason::NoDescent {
                    parameter: signatures[member].names[root].clone(),
                    callee: failing.callee.clone(),
                    argument: failing.arguments[position].clone(),
                }
            } else {
                FailureReason::PartialApplication {
                    callee: failing.callee.clone(),
                    position,
                }
            };
            Some(TerminationFailure {
                function: name.clone(),
                group: group.to_vec(),
                span: failing.span,
                reason,
            })
        })
        .collect()
}

/// Rejections for a group that *does* descend, but whose decreasing parameter
/// the search leaves undetermined.
pub(super) fn undetermined_failures(
    group: &[String],
    defs: &BTreeMap<String, DefView<'_>>,
    signatures: &[Signature<'_>],
    candidates: &[Vec<usize>],
    satisfying: &[Vec<usize>],
) -> Vec<TerminationFailure> {
    group
        .iter()
        .enumerate()
        .filter(|(member, name)| {
            defs[name.as_str()].annotation.decreasing.is_none()
                && satisfying
                    .iter()
                    .any(|assignment| assignment[*member] != satisfying[0][*member])
        })
        .map(|(member, _)| ambiguity_failure(group, signatures, candidates, member))
        .collect()
}

/// The "write the annotation down" rejection for one member.
pub(super) fn ambiguity_failure(
    group: &[String],
    signatures: &[Signature<'_>],
    candidates: &[Vec<usize>],
    member: usize,
) -> TerminationFailure {
    TerminationFailure {
        function: group[member].clone(),
        group: group.to_vec(),
        span: None,
        reason: FailureReason::AmbiguousDecreasing {
            candidates: candidates[member]
                .iter()
                .map(|&position| signatures[member].names[position].clone())
                .collect(),
        },
    }
}
