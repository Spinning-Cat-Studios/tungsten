//! Termination / admission error patterns (E0062, E0063 — ADRs 29.6.26e,
//! 11.8.26b, 12.8.26a).
//!
//! These had no entry at all until 11.8.26b's retrospective asked the tool the
//! obvious question and got "No matching diagnostic tools found". That mattered
//! more than the usual missing entry, because 11.8.26b also made termination a
//! **hard** gate: E0062 went from a warning most readers never saw to the error
//! a newcomer is most likely to hit first, and the mandated first step was blind
//! to it.
//!
//! **Keywords must be lowercase** — see the note in `runtime.rs`.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const TERMINATION_PATTERNS: &[ErrorPattern] = &[
    // ── E0062: a recursive definition the structural rule cannot certify ──
    //
    // Keywords span three vocabularies deliberately: the code and headline a
    // user pastes (`e0062`, "cannot prove termination"), the *reason* text the
    // diagnostic prints underneath it — which is what someone actually quotes
    // when the headline seems self-explanatory ("no parameter has an inductive
    // type", "not a known strict subterm") — and the words for the shape rather
    // than the message ("recursion", "decreasing", "structural").
    ErrorPattern {
        category: "termination",
        keywords: &[
            "e0062",
            "termination",
            "terminate",
            "cannot prove termination",
            "non-terminating",
            "structural recursion",
            "decreasing",
            "decreases",
            "strict subterm",
            "not a known strict subterm",
            "no parameter has an inductive type",
            "inductive type to descend on",
            "partial",
            "recursive group",
            "mutually recursive group",
            // Symptom (ADR 4.9.26d): the newcomer most likely to hit this gate
            // describes the rejection, not the rule — "it will not accept a
            // function that calls itself".
            "recursive function",
            "calls itself",
            "recursive definition",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check type termination <file> --verbose",
                cost: 3,
                reason: "The whole census rather than one rejection at a time: which groups certified, which are opaque through `#[partial]` taint, and which were refused. Reachable even when the gate would abort the build (ADR 12.8.26a)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten explain error E0062",
                cost: 1,
                reason: "The rule itself — what counts as a strict subterm, why reconstruction is not descent, and why a mutual group is measured against the CALLER's parameter",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info def <name> <file> --why-not-certified",
                cost: 3,
                reason: "Per-parameter: which are candidate decreasing roots and which are not, with the elaborated type that decided it. The answer to \"but it IS structural\" — usually the root's type is not what the source suggests",
                relevance: 0.92,
            },
            ToolSuggestion {
                command: "tungsten check <file> --termination proofs",
                cost: 3,
                reason: "Demote executable rejections to warnings to see them all at once, or to bisect. `report` silences the gate entirely; neither is how it should be left",
                relevance: 0.70,
            },
        ],
    },
    // ── E0063: a proof reached a `#[partial]` constant ──
    //
    // Separate from E0062 because the fix is different in kind: E0062 asks you
    // to change the recursion or opt out, E0063 says the opt-out has reached
    // somewhere it may not go, and no annotation on the proof can fix that.
    ErrorPattern {
        category: "termination-proof-boundary",
        keywords: &[
            "e0063",
            "partial in proof",
            "proof",
            "theorem",
            "lemma",
            "axiom",
            "tainted",
            "taint",
            "opaque constant",
            "cannot be used in proofs",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten explain error E0063",
                cost: 1,
                reason: "Why a `#[partial]` constant is inadmissible in a proof, and why a wrapper does not launder it — taint is transitive by design",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check type termination <file> --verbose",
                cost: 3,
                reason: "Prints the tainted set, which is the chain to walk: the proof does not name the partial constant it reached, and there is usually more than one hop between them",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info def <name> <file>",
                cost: 3,
                reason: "The elaborated Core term of the definition in the chain you suspect — the `#[partial]` may be reached through an argument rather than a call",
                relevance: 0.70,
            },
        ],
    },
];
