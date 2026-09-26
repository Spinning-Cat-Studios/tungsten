//! Enumerating assignments of one decreasing position per group member, and
//! testing whether an assignment satisfies every intra-group call
//! (ADR 29.6.26e §2.3).

use std::collections::BTreeMap;

use crate::terms::termination::size_env::CallSite;

/// All assignments of one candidate position per member that satisfy every
/// intra-group call.
pub(super) fn search_assignments(
    group: &[String],
    candidates: &[Vec<usize>],
    sites: &[BTreeMap<usize, Vec<CallSite>>],
) -> Vec<Vec<usize>> {
    let mut satisfying = Vec::new();
    let mut current = vec![0usize; group.len()];
    enumerate(candidates, 0, &mut current, &mut |assignment| {
        if assignment_holds(group, assignment, sites) {
            satisfying.push(assignment.to_vec());
        }
    });
    satisfying
}

/// Depth-first enumeration of the assignment product.
fn enumerate(
    candidates: &[Vec<usize>],
    depth: usize,
    current: &mut Vec<usize>,
    visit: &mut impl FnMut(&[usize]),
) {
    if depth == candidates.len() {
        visit(current);
        return;
    }
    for &position in &candidates[depth] {
        current[depth] = position;
        enumerate(candidates, depth + 1, current, visit);
    }
}

/// Whether every intra-group call descends under `assignment`.
fn assignment_holds(
    group: &[String],
    assignment: &[usize],
    sites: &[BTreeMap<usize, Vec<CallSite>>],
) -> bool {
    group.iter().enumerate().all(|(member, _)| {
        sites[member][&assignment[member]].iter().all(|site| {
            let Some(callee) = group.iter().position(|name| *name == site.callee) else {
                return true;
            };
            let position = assignment[callee];
            site.supplies(position) && site.descends_at(position)
        })
    })
}
