//! Elaboration / type-system error patterns: type mismatches, encoding,
//! mutual recursion, constructors, records, CIR variants and match dispatch.
//!
//! The module-**resolution** half — cross-file propagation, import aliases and
//! the private-item / name-collision class — lives in [`super::resolution`].
//! They were one table until ADR 13.8.26c's E0016 entry pushed this file to its
//! size limit, and the seam is real: these keywords are drawn from the *type*
//! checker's messages, those from the module walker's.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const ELABORATION_PATTERNS: &[ErrorPattern] = &[
    // ── Type mismatch ───────────────────────────────────────────────
    ErrorPattern {
        category: "type mismatch",
        keywords: &[
            "type mismatch", "expected type", "type error", "cannot unify",
            "incompatible type", "wrong type",
            // Symptom (4.9.26d): the identical-render case, where the two types
            // print the same and the reader can only say they look the same.
            "look identical", "look the same", "print the same",
            "do not line up",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info type-encoding <name> <file>",
                cost: 3,
                reason: "Display the μ-type encoding tree to understand structural types",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten diff types <a> <b> <file>",
                cost: 3,
                reason: "Structural tree-diff showing exactly where two types diverge",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten compile --trace-types=<name> <file>",
                cost: 3,
                reason: "Trace type transformations for a specific definition",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten compile --trace-normalization=<name> <file>",
                cost: 3,
                reason: "Trace normalization path to find where types diverge",
                relevance: 0.75,
            },
            ToolSuggestion {
                command: "tungsten info error-enrichment <file>",
                cost: 3,
                reason: "Show cross-file callers/callees to understand error propagation",
                relevance: 0.65,
            },
        ],
    },
    // ── Elaboration error ───────────────────────────────────────────
    ErrorPattern {
        category: "elaboration error",
        keywords: &[
            "elaboration error", "elab error", "elaboration failed",
            "not found", "undefined", "unresolved", "unknown constructor",
            "not an adt", "phase invariant",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten explain error <kind>",
                cost: 1,
                reason: "Explain what an elaboration error code means",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten compile --trace-types=<name> <file>",
                cost: 3,
                reason: "Trace type transformations to pinpoint elaboration issue",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten doctor check type integrity phase-invariants <file>",
                cost: 3,
                reason: "Catch phase-ordering bugs (e.g., TyVar escape, unresolved refs)",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten doctor check module signature-collection <file>",
                cost: 3,
                reason: "Check Signature Collection global collection health — import errors cause cascading E0001s",
                relevance: 0.75,
            },
        ],
    },
    // ── Encoding / μ-type issues ────────────────────────────────────
    ErrorPattern {
        category: "encoding / μ-type",
        keywords: &[
            "encoding", "mu-type", "μ-type", "mu type", "mu_var",
            "tyvar", "alpha_", "α_", "mu binder", "recursive type",
            // Symptom (4.9.26d): the type printed with something in it the
            // reader did not write, and cannot name.
            "binder",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info type-encoding <name> <file>",
                cost: 3,
                reason: "Display the full μ-type encoding tree for a named type",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten compile --dump-encoding=<name> <file>",
                cost: 3,
                reason: "Show encoding breakdown for an ADT",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten compile --trace-encoding=<name> <file>",
                cost: 3,
                reason: "Trace encoding decisions (stack, cycles, μ-vars)",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten doctor check type determinism normalization <file>",
                cost: 3,
                reason: "Detect normalization divergence in cached type encodings",
                relevance: 0.75,
            },
        ],
    },
    // ── Mutual recursion ────────────────────────────────────────────
    ErrorPattern {
        category: "mutual recursion",
        keywords: &[
            "mutual recursion", "mutually recursive", "scc", "cycle",
            "circular type", "circular dependency",
            // Symptom (4.9.26d): "two types that refer to each other" is the
            // shape a reader describes before knowing the word for it.
            "refer to each other", "reference each other",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info mutual-recursion-groups <file>",
                cost: 3,
                reason: "Show SCC groups and μ-binder order for all types",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor audit-mutual-types <file>",
                cost: 3,
                reason: "Full audit of mutually recursive type groups",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten doctor check fold-consistency <file>",
                cost: 3,
                reason: "Verify fold/unfold correctness for mutual recursion members",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten explain mutual-recursion",
                cost: 1,
                reason: "Understanding mutual type recursion and μ-encoding",
                relevance: 0.70,
            },
        ],
    },
    // ── Constructor / duplicate registration ────────────────────────
    ErrorPattern {
        category: "constructor / duplicate registration",
        keywords: &[
            "constructor", "duplicate registration", "duplicate constructor",
            "constructor count", "wrong total", "total=", "variant count",
            "ctor", "get_variant_payload",
            // Symptom (4.9.26d): a case appears twice, or the count reported
            // is not the number of cases that were written.
            "registered twice", "listed twice", "counted twice",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info constructors <name> <file>",
                cost: 3,
                reason: "Show constructor entries with duplicate detection for an ADT",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check type integrity constructor-counts <file>",
                cost: 3,
                reason: "Validate constructor-list integrity for all ADTs",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info adt <name> <file>",
                cost: 3,
                reason: "Show ADT details including constructor fields and encoding",
                relevance: 0.75,
            },
        ],
    },
    // ── Record field errors ─────────────────────────────────────────
    ErrorPattern {
        category: "record field",
        keywords: &[
            "record", "record type", "not a record", "missing field",
            "unknown field", "duplicate field", "extra field", "field access",
            "record constructor",
            // Symptom (4.9.26d): the field read back as a different field —
            // the observation the 3.9.26g projection bug presented as, which
            // also reaches the self-host divergence table by design.
            "wrong field", "reading a field", "struct member", "field comes back",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info type members record-fields <name> <file>",
                cost: 3,
                reason: "Show record fields with types and product encoding positions",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info type members field-type <Type.field> <file>",
                cost: 3,
                reason: "Show stored vs resolved type for a specific record field",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten explain error NotARecordType",
                cost: 1,
                reason: "Explain the 'not a record type' error",
                relevance: 0.70,
            },
        ],
    },
    // --- CIR variant lookup ---
    ErrorPattern {
        category: "cir variant lookup",
        keywords: &["cir", "variant", "constructor", "application", "site"],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info cir sites <variant> <file>",
                cost: 2,
                reason: "Find all CIR constructor application sites in module tree",
                relevance: 0.90,
            },
        ],
    },
    // --- Match dispatch / E0999 ---
    ErrorPattern {
        category: "match dispatch",
        keywords: &[
            "e0999", "match dispatch", "not a sum type", "not a product type",
            "dispatch_match", "tg_type_is_sum", "tg_type_is_mu",
            "cross-module match", "cross-module adt",
            "cannot instantiate polymorphic", "instantiate forall",
            "inner forall", "forall resolution",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check type integrity constructor-stubs <file>",
                cost: 3,
                reason: "Detect stale constructor stubs (TyVar instead of Sum encoding)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check type forall-resolution <file>",
                cost: 3,
                reason: "Detect inner foralls in structural positions that block extraction (ADR 21.5.26b)",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info type members constructors <name> <file>",
                cost: 3,
                reason: "Show constructor entries — check for duplicate/stub entries",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info type type-encoding <name> <file>",
                cost: 3,
                reason: "Check if ADT type encoding is a proper Sum/Product, not TyVar",
                relevance: 0.80,
            },
        ],
    },
    // ── Proof holes nobody wrote (ADR 18.9.26g) ─────────────────────
    ErrorPattern {
        category: "unexplained proof hole",
        keywords: &[
            "contains sorry", "sorry i didn't write", "sorry i did not write",
            "wrote no sorry", "no sorry in", "unreachable arm", "absurd branch",
            "which definition has the sorry",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check sorry-sites <file>",
                cost: 3,
                reason: "One row per definition carrying a hole: authored at file:line:col, synthesised by a nested pattern's lowering, or unclassified (`==` with no equality primitive) — no codegen",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info def <name> <file>",
                cost: 3,
                reason: "Read one named definition's Core term to see where its hole sits",
                relevance: 0.70,
            },
        ],
    },
    // ── Encoding nondeterminism / cross-run instability ─────────────
    ErrorPattern {
        category: "encoding nondeterminism",
        keywords: &[
            "nondeterministic", "non-deterministic", "flaps", "flapping",
            "flaky", "not reproducible", "run-to-run", "run to run",
            "different each run", "unstable encoding", "hashmap order",
            // Symptom (4.9.26d): "it changes between runs and I changed
            // nothing" — said before anyone suspects iteration order.
            "each run", "every run", "same input twice",
            "iteration order", "byte-unstable", "attempt count", "redo",
            "redoing work", "re-resolve", "resolution attempts",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check type encoding-determinism <file>",
                cost: 3,
                reason: "Elaborate twice and diff the stored encoding maps — is the Phase-1e output byte-stable across runs? (ADR 22.7.26c)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check type resolution-attempt-determinism <file>",
                cost: 3,
                reason: "Work-side twin: elaborate twice and diff the *count* of deferred type-reference resolution attempts — catches attempt-count flaps a results comparison misses (ADR 23.7.26a §6.1)",
                relevance: 0.8,
            },
            ToolSuggestion {
                command: "tungsten doctor check type determinism normalization <file> --raw-only",
                cost: 3,
                reason: "Compare stored vs fresh with raw == only — a regression canary for inline-depth instability; any raw-only divergence is a bug since ADR 22.7.26d",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info type type-encoding <name> <file>",
                cost: 3,
                reason: "Dump one type's stored encoding — hash across separate processes to check cross-process stability",
                relevance: 0.75,
            },
        ],
    },
];
