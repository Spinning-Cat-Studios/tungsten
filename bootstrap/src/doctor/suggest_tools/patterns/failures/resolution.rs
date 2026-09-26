//! Module-**resolution** error patterns: how a name fails to be found, or is
//! found in the wrong place.
//!
//! Split from [`super::elaboration`] (ADR 13.8.26c): those keywords are drawn
//! from the type checker's messages, these from the module walker's. The
//! private-item entry is the one that motivated the split — E0016 is reported
//! in the *loser's* file naming the *winner's* module, so its own text points
//! away from the edit that caused it, and a reader pasting it in is exactly the
//! reader who needs a tool named.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const RESOLUTION_PATTERNS: &[ErrorPattern] = &[
    // --- Cross-file / multi-module errors ---
    ErrorPattern {
        category: "cross-file error",
        keywords: &[
            "cross-file",
            "cross-module",
            "other file",
            "different file",
            "defined in",
            "imported from",
            "call site",
            "caller",
            "multi-file",
            "wrong module",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info error-enrichment <file>",
                cost: 3,
                reason: "Show cross-file call graph (incoming/outgoing) for error context",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info module imports <module> <file>",
                cost: 3,
                reason: "Inspect import resolution status for a module",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten info module alias-table <module> <file>",
                cost: 2,
                reason: "Check if a name was aliased away (imported under a different name)",
                relevance: 0.70,
            },
        ],
    },
    // --- Import aliasing / name resolution ---
    ErrorPattern {
        category: "import alias",
        keywords: &[
            "alias",
            "aliased",
            "as keyword",
            "use as",
            "renamed",
            "name resolution",
            "cannot find",
            "not in scope",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info module alias-table <module> <file>",
                cost: 2,
                reason: "Show alias mappings — aliased names suppress the original in scope",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info module imports <module> <file>",
                cost: 3,
                reason: "Inspect all import resolution status including aliases",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten explain error E0001",
                cost: 1,
                reason: "Explain the 'not found in scope' error",
                relevance: 0.70,
            },
        ],
    },
    // ── Private-item access (E0016) ─────────────────────────────────
    // The error is reported in the LOSER's file and names the WINNER's
    // module, so its own text points away from the edit that caused it
    // (ADR 13.8.26c §1.1). Keyworded on what the message actually says,
    // because that is what a reader pastes in.
    ErrorPattern {
        category: "private-item access / name collision",
        keywords: &[
            "is private",
            "e0016",
            "cannot be accessed from",
            "private and cannot be accessed",
            "name collision",
            "defined in two modules",
            "shadowed definition",
            // Symptom (ADR 4.9.26d): what a reader observes is that one of two
            // like-named functions stopped being callable — the collision is
            // the diagnosis, and E0016's own text points away from it.
            "same name",
            "became unreachable",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check module name-collisions <file>",
                cost: 2,
                reason: "Both compilers key values on the bare name: if two modules define \
                         one, the walk's last registration wins and the loser's call sites \
                         all report E0016. Names both modules and marks the winner — and \
                         runs on the file that fails to elaborate",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten explain error E0016",
                cost: 1,
                reason: "What the code means, before reading any source",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info error-sites E0016",
                cost: 1,
                reason: "Where the code is raised, by enclosing function — one code \
                         dominating a list is usually one boundary, not N faults",
                relevance: 0.70,
            },
            ToolSuggestion {
                command: "tungsten doctor check module reexport-completeness <file>",
                cost: 3,
                reason: "The dual question: if the name is NOT a collision, a pub use that \
                         copied nothing is the other way it goes missing",
                relevance: 0.60,
            },
        ],
    },
];
