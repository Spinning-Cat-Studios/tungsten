//! What a group may descend on: the indirect-occurrence veto, and the candidate
//! decreasing positions of each member (ADR 29.6.26e §2.2).

use std::collections::BTreeMap;

use super::{is_supported_inductive, Signature};
use crate::terms::termination::graph::OccurrenceGraph;
use crate::terms::termination::report::{FailureReason, RejectedRoot, TerminationFailure};
use crate::terms::termination::DefView;
use crate::terms::Term;
use crate::types::Type;

/// Reject group members mentioned outside call position.
pub(super) fn indirect_failures(
    group: &[String],
    graph: &OccurrenceGraph,
) -> Vec<TerminationFailure> {
    group
        .iter()
        .filter_map(|member| {
            let opaque = graph.opaque_uses(member)?;
            let reached = opaque.iter().find(|name| group.contains(name))?;
            Some(TerminationFailure {
                function: member.clone(),
                group: group.to_vec(),
                span: None,
                reason: FailureReason::IndirectOccurrence {
                    member: reached.clone(),
                },
            })
        })
        .collect()
}

/// The candidate decreasing positions of every member, or the rejections that
/// stopped the group from having any.
pub(super) fn candidate_positions(
    group: &[String],
    defs: &BTreeMap<String, DefView<'_>>,
    signatures: &[Signature<'_>],
) -> Result<Vec<Vec<usize>>, Vec<TerminationFailure>> {
    let mut candidates = Vec::with_capacity(group.len());
    let mut failures = Vec::new();

    for (member, name) in group.iter().enumerate() {
        let signature = &signatures[member];
        let reason = match defs[name].annotation.decreasing.as_deref() {
            Some(annotated) => annotated_candidate(annotated, signature).map(|position| {
                candidates.push(vec![position]);
            }),
            None => inferred_candidates(signature).map(|positions| {
                candidates.push(positions);
            }),
        };
        if let Err(reason) = reason {
            failures.push(TerminationFailure {
                function: name.clone(),
                group: group.to_vec(),
                span: None,
                reason,
            });
            candidates.push(Vec::new());
        }
    }

    if failures.is_empty() {
        Ok(candidates)
    } else {
        Err(failures)
    }
}

/// Resolve `#[decreasing(name)]` to a position.
fn annotated_candidate(annotated: &str, signature: &Signature<'_>) -> Result<usize, FailureReason> {
    let position = signature
        .names
        .iter()
        .position(|name| name == annotated)
        .ok_or_else(|| FailureReason::UnknownDecreasingParameter {
            annotated: annotated.to_string(),
            parameters: signature.names.clone(),
        })?;
    if is_supported_inductive(signature.types[position], &signature.own_type_parameters) {
        Ok(position)
    } else {
        Err(FailureReason::UnsupportedInductive {
            parameter: annotated.to_string(),
            rendered_type: signature.types[position].to_string(),
        })
    }
}

/// Every position whose type Phase 1 can descend on.
fn inferred_candidates(signature: &Signature<'_>) -> Result<Vec<usize>, FailureReason> {
    let positions: Vec<usize> = (0..signature.names.len())
        .filter(|&position| {
            is_supported_inductive(signature.types[position], &signature.own_type_parameters)
        })
        .collect();
    if positions.is_empty() {
        return Err(FailureReason::NoCandidateParameter {
            parameters: (0..signature.names.len())
                .map(|position| {
                    let ty = signature.types[position];
                    RejectedRoot {
                        name: signature.names[position].clone(),
                        rendered_type: ty.to_string(),
                        because: root_ineligibility(ty, &signature.own_type_parameters)
                            .unwrap_or("eligible"),
                    }
                })
                .collect(),
        });
    }
    Ok(positions)
}

/// Why Phase 1 will not descend on a parameter, or `None` when it will.
///
/// The *rendered type* alone does not answer this, which is the whole reason
/// this exists (ADR 12.8.26a). `Type`'s `Display` prints `TyVar("Param")` and a
/// nominal ADT named `Param` identically, so a diagnostic that echoed the type
/// showed `p: Param` for a parameter that was refused and `p: Param` for one
/// that was accepted. Naming the *variant class* is what distinguishes them —
/// and it is what would have shortened ADR 11.8.26b's investigation from four
/// source files to one command.
#[must_use]
pub fn root_ineligibility(ty: &Type, own_type_parameters: &[String]) -> Option<&'static str> {
    if is_supported_inductive(ty, own_type_parameters) {
        return None;
    }
    Some(match ty {
        Type::Nat
        | Type::Int
        | Type::Bool
        | Type::String
        | Type::Unit
        | Type::Void
        | Type::Prop => {
            "a primitive — it has no subterms, so a loop over it is a measure, not a descent"
        }
        Type::Arrow(..) => "a function type — there is no structure to take apart",
        Type::Ref(..) | Type::Ptr(..) => "a mutable cell — its contents can change between calls",
        Type::Product(..) | Type::Sum(..) => {
            "a structural product/sum; Phase 1 descends on named inductive types and μ-encodings"
        }
        // Reachable only for a *bound* TyVar: an unbound one is a nominal
        // cluster marker and `is_supported_inductive` already accepted it.
        Type::TyVar(..) => {
            "a type parameter of this definition — nothing is known about its shape here"
        }
        Type::App(..) => "an unresolved type application, left over from elaboration",
        Type::Eq(..) => "an equality witness, not an inductive value",
        Type::Forall(..) => "a polymorphic type; instantiate it before descending",
        Type::Error => "a poisoned type — fix the earlier error first",
        Type::Mu(..) | Type::Adt(..) => unreachable!("accepted above"),
    })
}

/// One parameter's eligibility as a decreasing root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCandidacy {
    /// The parameter's name, as written.
    pub parameter: String,
    /// Its elaborated type, rendered.
    pub rendered_type: String,
    /// `None` when it *is* a candidate; otherwise why it is not.
    pub ineligible_because: Option<&'static str>,
}

/// Every value parameter of `term`, with whether Phase 1 could descend on it.
///
/// Answers "but it *is* structural — why was it rejected?" without reading the
/// checker, which is the question ADR 11.8.26b spent four source files on.
/// Deliberately per-*definition* and independent of the group: a parameter's
/// eligibility is a property of its type alone, so this is meaningful even for
/// a definition the whole-project analysis never reached.
#[must_use]
pub fn describe_roots(term: &Term) -> Vec<RootCandidacy> {
    let signature = super::signature(term);
    (0..signature.names.len())
        .map(|position| {
            let ty = signature.types[position];
            RootCandidacy {
                parameter: signature.names[position].clone(),
                rendered_type: ty.to_string(),
                ineligible_because: root_ineligibility(ty, &signature.own_type_parameters),
            }
        })
        .collect()
}
