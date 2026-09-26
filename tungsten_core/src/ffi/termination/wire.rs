//! What crosses the seam in the failure direction (ADR 19.8.26d §2.3).
//!
//! Two formats, kept apart from the protocol in [`super::registry`] because a
//! stream that arrives out of order and a rejection that renders wrong are
//! different defects with different fixes.

use crate::terms::termination::{FailureReason, TerminationFailure};

use super::registry::Phase;

/// A call the protocol does not allow, with enough to say what to do instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProtocolError {
    /// A call made in the wrong phase.
    WrongPhase { call: &'static str, found: Phase },
    /// A payload offered for a name that was never declared.
    UndeclaredDefinition(String),
    /// A definition offered after the analysis already ran.
    AfterCheck(&'static str),
    /// A name argument that was null or not UTF-8.
    UnreadableName(&'static str),
    /// A handle that named nothing in the arena.
    UnreadablePayload(String),
}

impl ProtocolError {
    /// The message `tg_get_last_error` will carry.
    #[must_use]
    pub(crate) fn message(&self) -> String {
        match self {
            ProtocolError::WrongPhase { call, found } => format!(
                "termination mirror: `{call}` is not allowed in the {} phase",
                phase_label(*found)
            ),
            ProtocolError::UndeclaredDefinition(name) => format!(
                "termination mirror: `{name}` was never declared; the node set must be \
                 complete before any definition is reduced against it"
            ),
            ProtocolError::AfterCheck(call) => {
                format!("termination mirror: `{call}` after the analysis already ran")
            }
            ProtocolError::UnreadableName(call) => {
                format!("termination mirror: `{call}` was given an unreadable name")
            }
            ProtocolError::UnreadablePayload(name) => format!(
                "termination mirror: `{name}` was given a handle that names nothing in the arena"
            ),
        }
    }
}

/// The phase's name as an error message spells it.
fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Reduce => "reduce",
        Phase::Retain => "retain",
        Phase::Checked => "checked",
    }
}

/// Which self-hosted error kind a failure becomes.
///
/// The split is the bootstrap's: the proof boundary is its own diagnostic
/// (E0063 there, `PARTIAL_IN_PROOF` here) because "this proof reaches a partial
/// constant" and "this recursion does not descend" are different repairs.
pub(crate) const CANNOT_PROVE_TERMINATION: u64 = 0;
pub(crate) const PARTIAL_IN_PROOF: u64 = 1;

/// The wire kind of one failure.
#[must_use]
pub(crate) fn failure_kind(failure: &TerminationFailure) -> u64 {
    match failure.reason {
        FailureReason::PartialInProof { .. } => PARTIAL_IN_PROOF,
        _ => CANNOT_PROVE_TERMINATION,
    }
}

/// One failure as `"<kind>\n<function>\n<message>"`.
///
/// The kind and the function name travel *beside* the message rather than only
/// inside it because both are needed as DATA — the kind selects the error code,
/// the name finds the span to report at — and parsing either back out of an
/// English sentence would be a second thing that can disagree.
#[must_use]
pub(crate) fn render_failure(failure: &TerminationFailure) -> String {
    let mut message = failure.headline();
    for note in failure.notes() {
        message.push('\n');
        message.push_str(&note);
    }
    if let Some(help) = failure.suggestion() {
        message.push('\n');
        message.push_str(&help);
    }
    format!(
        "{}\n{}\n{}",
        failure_kind(failure),
        failure.function,
        message
    )
}
