//! What the termination checker concluded, and how it says so
//! (ADR 29.6.26e §2.6).
//!
//! Every failure names the recursive group, the decreasing parameter under
//! consideration, the offending argument and *why* that argument is not a known
//! strict subterm. The rendering lives here rather than in the elaborator's
//! error module so it is a pure function of the failure record, and so the
//! report-only tool and the gate cannot word the same rejection differently.

use std::collections::BTreeMap;

use crate::terms::TermSpan;

/// Where a definition stands with respect to the trusted environment
/// (ADR 29.6.26e §2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionState {
    /// Termination-checked (or not recursive at all): admitted as a reducible
    /// total constant.
    Total,
    /// `#[partial]`, or reached a `#[partial]` constant: admitted as an opaque
    /// constant, never δ-reduced, excluded from proofs.
    Partial,
    /// Not admitted to the trusted environment.
    Rejected,
}

impl AdmissionState {
    /// Whether the kernel may unfold this constant during conversion.
    ///
    /// The point of the staging: a definition is never δ-reducible before its
    /// SCC passes, so conversion cannot loop on an unchecked definition.
    #[must_use]
    pub fn is_delta_reducible(self) -> bool {
        self == AdmissionState::Total
    }

    /// Whether a trusted proof term or theorem statement may mention this.
    #[must_use]
    pub fn usable_in_proofs(self) -> bool {
        self == AdmissionState::Total
    }
}

/// A parameter that cannot be a decreasing root, and why (ADR 12.8.26a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedRoot {
    /// The parameter's name, as written.
    pub name: String,
    /// Its elaborated type, rendered.
    pub rendered_type: String,
    /// The class of type it is, in the terms the rule cares about.
    pub because: &'static str,
}

impl std::fmt::Display for RejectedRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}: {}` ({})",
            self.name, self.rendered_type, self.because
        )
    }
}

/// Why a definition could not be admitted as total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    /// The function has no parameter of an inductive type to descend on.
    NoCandidateParameter {
        /// Each parameter and why it was refused, in order; empty for a nullary
        /// definition. Carrying the *reason* rather than only the name is ADR
        /// 12.8.26a: `Display` renders a cluster-member marker and a real ADT
        /// identically, so a note that echoed names or even types left the
        /// reader unable to tell an unsupported type from a mis-encoded one.
        parameters: Vec<RejectedRoot>,
    },
    /// A group member is mentioned somewhere other than call position, so the
    /// recursion cannot be resolved into an inspectable call-graph edge.
    IndirectOccurrence {
        /// The group member reached opaquely.
        member: String,
    },
    /// A recursive call leaves the callee partially applied, so it never
    /// consumes its decreasing argument.
    PartialApplication {
        /// The group member called.
        callee: String,
        /// The decreasing position it never receives.
        position: usize,
    },
    /// No choice of decreasing parameter satisfies every recursive call.
    NoDescent {
        /// The parameter under consideration when the call was rejected.
        parameter: String,
        /// The group member called.
        callee: String,
        /// The argument supplied in the callee's decreasing position.
        argument: String,
    },
    /// More than one parameter satisfies every recursive call, so the choice
    /// has to be written down.
    AmbiguousDecreasing {
        /// The parameters that would each work.
        candidates: Vec<String>,
    },
    /// `#[decreasing(x)]` names something that is not a parameter.
    UnknownDecreasingParameter {
        /// The name written in the annotation.
        annotated: String,
        /// The parameters that do exist.
        parameters: Vec<String>,
    },
    /// The decreasing parameter's type is outside the Phase-1 supported subset.
    UnsupportedInductive {
        /// The annotated or sole candidate parameter.
        parameter: String,
        /// Its type, rendered.
        rendered_type: String,
    },
    /// A proof reached a tainted constant (ADR 29.6.26e §2.4).
    PartialInProof {
        /// The `#[partial]` constant at the end of the chain.
        tainted: String,
        /// Intermediate constants the taint travelled through, nearest first;
        /// empty when the proof mentions the partial constant directly.
        via: Vec<String>,
    },
}

/// One rejected definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminationFailure {
    /// The definition that could not be admitted.
    pub function: String,
    /// Its recursive group, sorted; a single name for a self-recursive
    /// definition, empty for a taint failure (which is not about recursion).
    pub group: Vec<String>,
    /// Where to point the diagnostic, when a specific call site is at fault.
    pub span: Option<TermSpan>,
    /// Why.
    pub reason: FailureReason,
}

impl TerminationFailure {
    /// The one-line message.
    #[must_use]
    pub fn headline(&self) -> String {
        match &self.reason {
            FailureReason::PartialInProof { tainted, .. } => format!(
                "proof `{}` depends on the partial constant `{tainted}`",
                self.function
            ),
            _ => format!("cannot prove termination of `{}`", self.function),
        }
    }

    /// The explanatory notes, in the order they should be printed.
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.group.len() > 1 {
            notes.push(format!(
                "mutually recursive group: {}",
                self.group.join(", ")
            ));
        }
        notes.push(self.reason_note());
        notes
    }

    /// The `help:` line, when there is an actionable one.
    #[must_use]
    pub fn suggestion(&self) -> Option<String> {
        match &self.reason {
            FailureReason::AmbiguousDecreasing { candidates } => Some(format!(
                "annotate the intended parameter, e.g. `#[decreasing({})]`",
                candidates.first().map_or("arg", String::as_str)
            )),
            FailureReason::NoCandidateParameter { .. }
            | FailureReason::NoDescent { .. }
            | FailureReason::UnsupportedInductive { .. } => Some(
                "rewrite the recursion structurally, or mark the definition \
                 `#[partial]` (partial definitions cannot be used in proofs)"
                    .to_string(),
            ),
            FailureReason::IndirectOccurrence { .. } | FailureReason::PartialApplication { .. } => {
                Some(
                    "call the definition directly so the recursion is visible, \
                     or mark it `#[partial]`"
                        .to_string(),
                )
            }
            FailureReason::UnknownDecreasingParameter { parameters, .. } => Some(format!(
                "`#[decreasing(…)]` must name a parameter: {}",
                render_list(parameters)
            )),
            FailureReason::PartialInProof { .. } => Some(
                "a proof may not depend on a partial definition — prove the \
                 recursion terminates, or state the theorem about a total one"
                    .to_string(),
            ),
        }
    }

    /// The reason as a single note line.
    fn reason_note(&self) -> String {
        match &self.reason {
            FailureReason::NoCandidateParameter { parameters } if parameters.is_empty() => {
                "the definition takes no parameters, so nothing can decrease".to_string()
            }
            FailureReason::NoCandidateParameter { parameters } => format!(
                "no parameter has an inductive type to descend on: {}",
                parameters
                    .iter()
                    .map(RejectedRoot::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FailureReason::IndirectOccurrence { member } => format!(
                "`{member}` is mentioned outside call position, so the recursive \
                 call cannot be inspected"
            ),
            FailureReason::PartialApplication { callee, position } => format!(
                "`{callee}` is applied to too few arguments to reach its \
                 decreasing parameter (position {position})"
            ),
            FailureReason::NoDescent {
                parameter,
                callee,
                argument,
            } => format!(
                "with `{parameter}` decreasing, the call to `{callee}` supplies \
                 `{argument}`, which is not a known strict subterm of `{parameter}`"
            ),
            FailureReason::AmbiguousDecreasing { candidates } => format!(
                "more than one parameter decreases in every recursive call: {}",
                render_list(candidates)
            ),
            FailureReason::UnknownDecreasingParameter { annotated, .. } => {
                format!("`{annotated}` is not a parameter of this definition")
            }
            FailureReason::UnsupportedInductive {
                parameter,
                rendered_type,
            } => format!(
                "`{parameter}` has type `{rendered_type}`, which is not a \
                 simple inductive type Phase 1 can descend on"
            ),
            FailureReason::PartialInProof { tainted, via } if via.is_empty() => {
                format!("`{tainted}` is marked `#[partial]`")
            }
            FailureReason::PartialInProof { tainted, via } => format!(
                "reaches `{tainted}` (marked `#[partial]`) through {}",
                via.join(" → ")
            ),
        }
    }
}

/// Everything one termination pass concluded.
#[derive(Debug, Default)]
pub struct TerminationReport {
    /// Rejections, in deterministic order (group order, then member order).
    pub failures: Vec<TerminationFailure>,
    /// Final admission state of every definition.
    pub admission: BTreeMap<String, AdmissionState>,
    /// The recursive groups that were checked, sorted.
    pub recursive_groups: Vec<Vec<String>>,
    /// Definitions carrying taint, whether annotated or inherited.
    pub tainted: Vec<String>,
}

impl TerminationReport {
    /// Whether every definition was admitted.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    /// The admission state of `name`; an unknown name is not admitted.
    #[must_use]
    pub fn state_of(&self, name: &str) -> AdmissionState {
        self.admission
            .get(name)
            .copied()
            .unwrap_or(AdmissionState::Rejected)
    }
}

/// Render a name list as `` `a`, `b`, `c` `` (or `none` when empty).
fn render_list(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_string();
    }
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
