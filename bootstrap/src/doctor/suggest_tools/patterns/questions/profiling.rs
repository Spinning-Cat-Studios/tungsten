//! Profiling / cost-attribution patterns (ADR 5.8.26b retrospective).
//!
//! These are not *error* patterns — they are the questions asked when the
//! compiler is correct but slow, or when a profile is being read. They earn a
//! place in `suggest-tools` for the same reason the silent-Stuck class does:
//! the user has no error text to paste, so the tool that would answer them is
//! the hardest one to find.
//!
//! **Keywords must be lowercase** — see `patterns/mod.rs`; `scored_patterns`
//! lowercases the description before `contains`, so an uppercase keyword is
//! dead weight that reads like a working synonym.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const PROFILING_PATTERNS: &[ErrorPattern] = &[
    // ── Where is the self-compiled check spending time? ──────────────
    ErrorPattern {
        category: "compile-time hotspot",
        keywords: &[
            "slow",
            "hotspot",
            "hot spot",
            "profile",
            "profiling",
            "perf",
            "cpu time",
            "wall clock",
            // "too long" rather than "takes too long" (ADR 4.9.26d): the
            // longer form is a strict superset, so it matched a narrower set of
            // real phrasings ("takes far too long") for no gain.
            "too long",
            "compile time",
            "where is the time",
            "quadratic",
            "o(n^2)",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "make devcontainer-profile-selfcompiled",
                cost: 5,
                reason: "CPU-profile the self-compiled check (tungsten1 + perf). NOT `make profile`, which samples the Rust bootstrap — compiled .tg symbols never execute there, so their shares read ~0%",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info codegen symbols <file> --by-function <fn>",
                cost: 4,
                reason: "Every symbol one .tg function compiles to — sum a profile's self time across ALL of them; `<name>` + `<name>$direct` alone under-counts",
                relevance: 0.9,
            },
            ToolSuggestion {
                command: "tungsten doctor check unit-cost <file>",
                cost: 4,
                reason: "Ranked per-unit codegen cost census (wall time + allocation volume)",
                relevance: 0.7,
            },
            ToolSuggestion {
                command: "tungsten-dev selfcompiled-profile",
                cost: 5,
                reason: "HEAP profile, not CPU: per-module RSS deltas by allocation class — the other half of the picture",
                relevance: 0.6,
            },
        ],
    },
    // ── Reading a profile: which symbol IS this function? ────────────
    ErrorPattern {
        category: "profile symbol attribution",
        keywords: &[
            "symbol",
            "direct_mt",
            "$direct",
            "lambda_",
            "nm output",
            "perf report",
            "attribute",
            "attribution",
            "which symbol",
            "mangled",
            // Symptom (ADR 4.9.26d): the profile is full of names that are not
            // in the source, and the reader cannot yet say why there are four.
            "not in my source",
            "do not recognise",
            "do not recognize",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info codegen symbols <file> --by-function <fn>",
                cost: 4,
                reason: "One .tg function compiles to up to four symbol KINDS (`<name>`, `$direct`, `$direct_mt`, `_lambda_N`); this lists the set with each one's role",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info codegen symbols <file>",
                cost: 4,
                reason: "The whole lambda → source-name map, when the question is 'what is __lambda_7?'",
                relevance: 0.75,
            },
            ToolSuggestion {
                command: "tungsten info codegen musttail-eligibility <fn> <file>",
                cost: 4,
                reason: "Why a function has (or lacks) a `$direct_mt` — the musttail/indirect-buffer decision that creates it",
                relevance: 0.6,
            },
        ],
    },
    // ── "Is this violation mine?" ────────────────────────────────────
    ErrorPattern {
        category: "change-scoped quality gate",
        keywords: &[
            "pre-existing",
            "preexisting",
            "is this mine",
            "my change",
            "new violation",
            "violation",
            "check-health",
            "code health",
            "scoped",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "make check-health-diff",
                cost: 1,
                reason: "Health findings scoped to what this branch changed — separates new violations from pre-existing ones without a second checkout (SINCE=<ref> to retarget)",
                relevance: 0.9,
            },
            ToolSuggestion {
                command: "make coverage-diff-gate",
                cost: 3,
                reason: "The same scout's-rule scoping for line coverage of the changed lines",
                relevance: 0.6,
            },
            ToolSuggestion {
                command: "make mutants-diff",
                cost: 5,
                reason: "The same scoping for mutation coverage — survivors in the changed lines",
                relevance: 0.6,
            },
        ],
    },
];
