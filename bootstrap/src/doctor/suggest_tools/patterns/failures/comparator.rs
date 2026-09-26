//! Structural-comparator error patterns (ADR 1.8.26b).
//!
//! Split out of `runtime.rs` for the file-size limit, and it reads better here:
//! the defining symptom of this class is NOT a crash or an error message but a
//! test that PASSES when it should fail, so the keywords cover how a user
//! actually describes it as well as the internal vocabulary.
//!
//! **Keywords must be lowercase** — see the note in `runtime.rs`.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const COMPARATOR_PATTERNS: &[ErrorPattern] = &[
    // ── Structural comparator: silently Stuck compare / vacuous assert ──
    //
    // The defining symptom is NOT an error message — it is a test that passes
    // when it should fail. `compare<T>` at a type whose comparator cannot be
    // synthesized leaves the call Stuck, the enclosing assert never runs, and
    // the runner reports `ok`. So the keywords cover how a user actually
    // describes it ("assert_eq passes", "test passes but shouldn'''t") as well
    // as the internal vocabulary (`__cmp`, `compare_`, "stuck").
    ErrorPattern {
        category: "comparator-stuck",
        keywords: &[
            "compare", "comparator", "__cmp", "compare_", "assert_eq", "assert_ne",
            "stuck", "residual term", "vacuous", "passes but should fail",
            "test passes", "asserts nothing", "structural equality",
            // Symptom (ADR 4.9.26d): the suite is green and the reader does not
            // believe it — before they can name the Stuck compare underneath.
            "checked anything", "green but", "did not check",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check comparable <type> <file>",
                cost: 3,
                reason: "Can the comparator handle this type? A compare it cannot synthesize goes Stuck, so the assert never runs and the test reports ok",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info type size <type> <file>",
                cost: 3,
                reason: "Reports the mu-unfold factor; a large product is why synthesis does not terminate",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten info eval trace <def> <file>",
                cost: 3,
                reason: "Is it looping, growing, or stuck? Per-step node counts on the evaluator",
                relevance: 0.75,
            },
            ToolSuggestion {
                command: "tungsten test <file> --assertion-census",
                cost: 3,
                reason: "How many assertions each test actually EXECUTED. A zero is proof it asserted nothing, whatever the cause — and unlike the comparator check it needs no guess at which type is at fault (ADR 6.8.26b)",
                relevance: 0.97,
            },
            ToolSuggestion {
                command: "tungsten info eval externs",
                cost: 1,
                reason: "The OTHER way an assertion silently never runs: a sub-expression reaching an extern absent from this list goes Stuck, so the assert is never evaluated and there are no operands for the comparator guard to inspect (ADR 6.8.26b)",
                relevance: 0.85,
            },
        ],
    },
];
