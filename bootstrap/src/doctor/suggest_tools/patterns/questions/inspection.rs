//! "Show me X" patterns — questions with no error text at all.
//!
//! The registry's other tables are keyed on *failures*: something went wrong and
//! you have a message to paste. This one is keyed on the question you ask when
//! nothing is wrong and you simply need to see what the compiler built — the
//! shape of an elaborated term, a definition's inferred type, what a `match`
//! lowered to. `profiling` was split out for the same reason (ADR 5.8.26b: the
//! correct-but-slow questions "with no error text to paste, which is exactly why
//! they are hard to find"); these are the correct-but-opaque ones.
//!
//! It exists because of a measured miss. ADR 29.6.26e needed the Core shape of
//! an elaborated `match`, and `tungsten info def` answers that exactly, at cost
//! 3, with no codegen feature. Every phrasing of the question — "core term",
//! "core ir", "inspect a definition", "what does an elaborated function look
//! like" — returned *no matches*, so the session hand-rolled a throwaway
//! printing `#[test]` instead and deleted it afterwards.
//!
//! Ranked last in `all_patterns()`: a description that mentions both a crash and
//! a term dump must still rank the crash tools first.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const INSPECTION_PATTERNS: &[ErrorPattern] = &[
    // ── What did this definition elaborate to? ──────────────────────
    ErrorPattern {
        category: "inspect a definition",
        keywords: &[
            "core term",
            "core ir",
            "elaborated term",
            "elaborated form",
            "what does it elaborate to",
            "inspect a definition",
            "show me the definition",
            "dump the term",
            "term shape",
            "lowered to",
            "desugars to",
            "inferred type of",
            // Symptom (ADR 4.9.26d): "what did my function turn into" is the
            // question; "Core term" is the vocabulary for the answer.
            "turned into",
            "what it became",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info def <name> <file>",
                cost: 3,
                reason: "Semantic type, structural type AND the full Core term — no codegen feature needed. Use this instead of writing a printing test",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info try-desugar <name> <file>",
                cost: 3,
                reason: "How the `?` operator expanded in a specific definition",
                relevance: 0.70,
            },
            ToolSuggestion {
                command: "tungsten compile --dump-ir=<name> <file>",
                cost: 4,
                reason: "Pretty-printed Core IR on the compile path — needs the codegen feature; prefer `info def` when you only want to read the term",
                relevance: 0.55,
            },
        ],
    },
    // ── What does a match/ADT look like once encoded? ───────────────
    ErrorPattern {
        category: "inspect an encoding",
        keywords: &[
            "what does a match lower to",
            "lower to",
            "lowers to",
            "match lowering",
            "adt shape",
            "constructor index",
            "sum encoding",
            "which constructor is inl",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info type members constructors <name> <file>",
                cost: 3,
                reason: "Constructor order and payload types — settles inl/inr without guessing",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info type adt <name> <file>",
                cost: 3,
                reason: "The ADT's encoded Sum/Product/μ structure",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info def <name> <file>",
                cost: 3,
                reason: "The lowered `Case`/`AdtMatch` term for a function that matches on it (see repo-memory elaboration-pipeline.md § What a `match` lowers to)",
                relevance: 0.80,
            },
        ],
    },
];
