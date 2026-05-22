//! Static pattern registry mapping error keywords to diagnostic tool suggestions.
//!
//! Each `ErrorPattern` maps a set of keywords to ranked tool suggestions.
//! The matching engine scores patterns by keyword overlap and deduplicates
//! suggestions across multiple matching patterns.

use super::{ErrorPattern, ToolSuggestion};

pub(super) const PATTERNS: &[ErrorPattern] = &[
    // ── Segfault / SIGSEGV ──────────────────────────────────────────
    ErrorPattern {
        category: "segfault",
        keywords: &[
            "sigsegv", "segfault", "segmentation fault", "signal 11",
            "null pointer", "access violation",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check fold-consistency <file>",
                cost: 3,
                reason: "SIGSEGV often caused by fold/unfold mismatch in recursive ADTs",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check ir-layout <file.ll>",
                cost: 1,
                reason: "Detect store/load type-width mismatches in LLVM IR",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info adt <name> <file> --check-fold",
                cost: 3,
                reason: "Check fold/unfold consistency for a specific ADT",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten info mutual-recursion-groups <file>",
                cost: 3,
                reason: "Verify mutual recursion detection is correct",
                relevance: 0.70,
            },
        ],
    },
    // ── Type mismatch ───────────────────────────────────────────────
    ErrorPattern {
        category: "type mismatch",
        keywords: &[
            "type mismatch", "expected type", "type error", "cannot unify",
            "incompatible type", "wrong type",
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
    // ── Stack overflow ──────────────────────────────────────────────
    ErrorPattern {
        category: "stack overflow",
        keywords: &[
            "stack overflow", "stack exhaustion", "thread.*overflowed",
            "deep recursion", "recursion limit",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check link-health <binary>",
                cost: 1,
                reason: "Verify binary has correct stack size (ld64.lld silently ignores -stack_size)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten explain stack-overflow",
                cost: 1,
                reason: "Quick reference on stack overflow causes and solutions",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten doctor audit-recursion <file>",
                cost: 3,
                reason: "Classify recursive functions (tail, tree, linear, general)",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten doctor check encoding-depth <file>",
                cost: 3,
                reason: "Check for runaway type-term depth that may cause stack overflow",
                relevance: 0.80,
            },
        ],
    },
    // ── Infinite loop / hang ────────────────────────────────────────
    ErrorPattern {
        category: "infinite loop",
        keywords: &[
            "infinite loop", "hang", "not terminating", "stuck",
            "never returns", "timeout", "timed out",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor audit-recursion <file>",
                cost: 3,
                reason: "Identify non-tail recursive functions that may not terminate",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten explain recursion-types",
                cost: 1,
                reason: "Classification of recursion patterns and termination risks",
                relevance: 0.75,
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
                command: "tungsten doctor check phase-invariants <file>",
                cost: 3,
                reason: "Catch phase-ordering bugs (e.g., TyVar escape, unresolved refs)",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten doctor check phase-a5 <file>",
                cost: 3,
                reason: "Check Phase A.5 global collection health — import errors cause cascading E0001s",
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
                command: "tungsten doctor check normalization-consistency <file>",
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
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info constructors <name> <file>",
                cost: 3,
                reason: "Show constructor entries with duplicate detection for an ADT",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check constructor-counts <file>",
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
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info type record-fields <name> <file>",
                cost: 3,
                reason: "Show record fields with types and product encoding positions",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten info type field-type <Type.field> <file>",
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
    // --- ABI mismatch ---
    ErrorPattern {
        category: "abi mismatch",
        keywords: &["abi", "layout", "emitter", "abi mismatch"],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten diff abi <type> <file>",
                cost: 3,
                reason: "Compare ABI layout between bootstrap codegen and .tg emitter",
                relevance: 0.95,
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
    // --- Cross-file / multi-module errors ---
    ErrorPattern {
        category: "cross-file error",
        keywords: &[
            "cross-file", "cross-module", "other file", "different file",
            "defined in", "imported from", "call site", "caller",
            "multi-file", "wrong module",
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
            "alias", "aliased", "as keyword", "use as", "renamed",
            "name resolution", "cannot find", "not in scope",
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
    // --- Match dispatch / E0999 ---
    ErrorPattern {
        category: "match dispatch",
        keywords: &[
            "E0999", "match dispatch", "not a sum type", "not a product type",
            "dispatch_match", "tg_type_is_sum", "tg_type_is_mu",
            "cross-module match", "cross-module ADT",
            "cannot instantiate polymorphic", "instantiate forall",
            "inner forall", "forall resolution",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check type constructor-stubs <file>",
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
                command: "tungsten info type constructors <name> <file>",
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
    // ── Linker / self-compile issues ────────────────────────────────
    ErrorPattern {
        category: "linker / self-compile",
        keywords: &[
            "linker", "link error", "duplicate symbol", "undefined symbol",
            "self-compile", "tungsten1", "tungsten2", "L3", "L4",
            "lld", "ld64", "stack_size", "case-insensitive",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check link-health <binary>",
                cost: 1,
                reason: "Verify binary stack size and executability",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check self-compile-readiness",
                cost: 1,
                reason: "Pre-flight checks for self-compile (filesystem, linker, LLVM)",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten doctor check link-collisions <dir>",
                cost: 1,
                reason: "Check for duplicate symbols across object files",
                relevance: 0.80,
            },
        ],
    },
    // ── Nested pattern / unknown value in match ─────────────────────
    ErrorPattern {
        category: "nested pattern",
        keywords: &[
            "unknown value", "nested pattern", "tuple pattern",
            "Ok((", "Err((", "Some((", "constructor tuple",
            "nested match", "nested constructor", "pattern binding",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check nested-patterns <file>",
                cost: 2,
                reason: "Scan AST for Ctor((a, b)) patterns that tungsten1 miscompiles (ADR 20.5.26a)",
                relevance: 0.98,
            },
            ToolSuggestion {
                command: "tungsten info def <name> <file>",
                cost: 3,
                reason: "Inspect Core IR to verify pattern destructuring binds variables correctly",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor map-span <file> <offset>",
                cost: 1,
                reason: "Map byte offset to file:line:col for errors with column-only positions",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten info type constructors <name> <file>",
                cost: 3,
                reason: "Verify constructor index (source-order: 0=left/inl, 1=right/inr)",
                relevance: 0.80,
            },
        ],
    },
    // ── L2/L3 self-host divergence ─────────────────────────────────
    ErrorPattern {
        category: "L1/L2 divergence",
        keywords: &[
            "L3", "L2", "tungsten1", "self-compiled", "self-host",
            "L1 passes L2 fails", "codegen regression", "l1-l2",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten diff l1-l2-check <file> --l2-binary ./tungsten1",
                cost: 6,
                reason: "Compare L1 vs tungsten1 elaboration — shows error count diff and L2-only errors",
                relevance: 0.98,
            },
            ToolSuggestion {
                command: "tungsten doctor check nested-patterns <file>",
                cost: 2,
                reason: "Detect nested ctor+tuple patterns that tungsten1 miscompiles",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten explain error --l2 <code>",
                cost: 1,
                reason: "Decode L2 error codes (different numbering from L1)",
                relevance: 0.85,
            },
        ],
    },
];
