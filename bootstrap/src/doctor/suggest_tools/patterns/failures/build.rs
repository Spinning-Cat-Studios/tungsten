//! Build-side patterns: linking, self-compile, symbol resolution and the
//! compiler's own failures.
//!
//! Split from `runtime` on the seam between *the program misbehaves* (a
//! segfault, a wrong value, a loop) and *the build misbehaves* (it will not
//! link, it resolves the wrong symbol, it never finishes). The two families
//! share almost no tools, and a reader scanning for one was reading past the
//! other.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const BUILD_PATTERNS: &[ErrorPattern] = &[
    // ── Linker / self-compile issues ────────────────────────────────
    ErrorPattern {
        category: "linker / self-compile",
        keywords: &[
            "linker", "link error", "duplicate symbol", "undefined symbol",
            "self-compile", "tungsten1", "tungsten2", "self-compiled",
            // Legacy search synonyms (ADR 24.7.26e): "l3" is the retired role
            // label; "l4" is the ADR 9.2.26 ladder rung (generation axis).
            // Lowercase because the matcher lowercases the QUERY only.
            "l3", "l4",
            "lld", "ld64", "stack_size", "case-insensitive",
            // Symptom (4.9.26d): everything compiled and the very last step
            // failed — the user has no idea yet that the linker is involved.
            "fails at the end", "last step", "building the compiler",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check link health <binary>",
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
                command: "tungsten doctor check link collisions <dir>",
                cost: 1,
                reason: "Check for duplicate symbols across object files",
                relevance: 0.80,
            },
        ],
    },
    // ── Synthesized symbol referenced but not declared (ADR 1.7.26f) ─
    // A mono-instance calling a global the depot never declared surfaces as a
    // link/codegen "referenced but not declared" error; 12.7.26c wires its
    // vocabulary (the 1.7.26f retrospective).
    ErrorPattern {
        category: "referenced but not declared",
        keywords: &[
            "referenced but not declared", "not declared", "missing declaration",
            "no declaration for", "referenced but undeclared",
            // Symptom (4.9.26d): a name is used in the output and defined
            // nowhere in it — observed as a build that names a function nobody
            // wrote out.
            "no definition", "never defined", "never emitted",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten info codegen symbols <file>",
                cost: 4,
                reason: "List emitted symbols to see whether the referenced global was declared/defined at all (ADR 1.7.26f)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check link collisions <dir>",
                cost: 1,
                reason: "Cross-object symbol audit — catches a missing or duplicated declaration across codegen units",
                relevance: 0.85,
            },
        ],
    },
    // ── Colliding-name extern-map misresolution (ADR 12.7.26b) ──────
    ErrorPattern {
        category: "colliding-name misresolution",
        keywords: &[
            "wrong function called", "colliding name", "wrong module",
            "import resolves to wrong module", "calls the wrong", "extern map",
            "clobber", "same-named function",
            // Symptom (4.9.26d): the call ran and did the wrong thing. The
            // user sees two definitions sharing a name, not a clobber.
            "wrong function", "same name", "different module",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check extern-map-ambiguity <file>",
                cost: 4,
                reason: "Enumerate every (unit, name) whose reference resolves against ≥2 same-named defs — names the clobber-last pick (ADR 12.7.26b)",
                relevance: 0.97,
            },
            ToolSuggestion {
                command: "tungsten diff exec <file>",
                cost: 5,
                reason: "Confirm the misresolution at runtime: evaluator vs native output comparison",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten info module imports <module> <file>",
                cost: 3,
                reason: "Inspect which def an import actually resolves to",
                relevance: 0.75,
            },
        ],
    },
    // ── bootstrap-vs-self-host divergence ──────────────────────────
    //
    // The pattern ADR 4.9.26d was written about. Every keyword below the legacy
    // block already contained the *conclusion*: a reader who could type
    // "self-host divergence" did not need the tool, and the reader who could
    // only say "the field came back wrong" — the literal symptom of the 3.9.26g
    // projection bug — got nothing and started reading source, which is the
    // outcome the mandated first step exists to prevent.
    ErrorPattern {
        category: "bootstrap/self-host divergence",
        keywords: &[
            "tungsten1", "self-compiled", "self-host",
            "bootstrap passes self-host fails", "codegen regression",
            "bootstrap-selfhost", "free variable", "unbound variable",
            "prints the term instead of the value", "core term differs",
            "not a value", "stuck term", "does not reduce",
            // Symptom (4.9.26d): the observations from the 3.9.26g
            // investigation, none of which name the divergence.
            "wrong field", "wrong element", "destructuring", "projection",
            "one compiler", "compilers disagree", "passes one and fails",
            // Legacy search synonyms (ADR 24.7.26e) — matcher inputs only.
            // Lowercase because the matcher lowercases the QUERY only.
            "l3", "l2", "l1 passes l2 fails", "l1-l2",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten diff bootstrap-selfhost-check <file> --selfhost-binary ./tungsten1",
                cost: 6,
                reason: "Compare bootstrap vs tungsten1 elaboration — shows error count diff and self-host-only errors",
                relevance: 0.98,
            },
            ToolSuggestion {
                command: "tungsten diff selfhost-core <def> <file>",
                cost: 6,
                reason: "Compare ONE definition's Core TERM across both compilers — the divergence a verdict comparison cannot see (ADR 19.8.26d)",
                relevance: 0.94,
            },
            ToolSuggestion {
                command: "tungsten doctor check selfhost closed-terms <file>",
                cost: 3,
                reason: "Census of self-hosted bodies with FREE variables — the shape that makes `tungsten1 run` print a term and defeats descent",
                relevance: 0.92,
            },
            ToolSuggestion {
                command: "tungsten doctor check selfhost well-typed-terms <file>",
                cost: 3,
                reason: "The OTHER reason `tungsten1 run` prints a term: an eliminator over the wrong former. Closed, type-checks, and closed-terms passes it (ADR 3.9.26h)",
                relevance: 0.92,
            },
            ToolSuggestion {
                command: "tungsten doctor check nested-patterns <file>",
                cost: 2,
                reason: "Detect nested ctor+tuple patterns that tungsten1 miscompiles",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten explain error --self-hosted <code>",
                cost: 1,
                reason: "Decode self-host error codes (different numbering from the bootstrap)",
                relevance: 0.85,
            },
        ],
    },
    // ── Stale / warm elaboration cache (ADR 4.7.26c) ─────────────────
    // A warm-cache `run`/`test` reading a bodyless signature-only elab-cache
    // entry (written by a prior `check`) yields empty defs → `run` can't find
    // `main` (spurious E0030 whose EOF span prints "span out of bounds") and
    // `test` prints "no tests found". This class had no suggest-tools pattern,
    // so the mandated diagnostic-first step returned nothing (which is how the
    // superseded ADR 3.7.26f mis-diagnosed it as file-path tracking).
    ErrorPattern {
        category: "stale/warm elaboration cache",
        keywords: &[
            "no tests found", "no main function", "no `main`",
            "empty defs", "0 definitions", "warm cache", "stale cache",
            "cache poisoning", "span out of bounds", "file_path tracking",
            // Symptom (4.9.26d): the tell is the SECOND run, and the user
            // reports the sequence rather than the cache.
            "second run", "second time", "finds nothing", "worked the first time",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten cache clean",
                cost: 1,
                reason: "Clear stale/bodyless elab-cache entries — a warm run/test reading a check-written signature cache is the usual cause of empty defs / 'no main' / 'no tests found' (ADR 4.7.26c)",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten cache inspect <file> --mode run",
                cost: 3,
                reason: "Confirm the cause before clearing: shows each module's cache tier + whether the entry would serve run/test bodies — a signature-only 'NO' row is the exact 4.7.26c hazard (ADR 4.7.26d)",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten diff cache <file>",
                cost: 5,
                reason: "Run the file cold then warm and compare — auto-detects the cache-poisoning divergence class (cold ok, warm 'no tests found'); pairs with --gate as a CI canary (ADR 4.7.26d)",
                relevance: 0.85,
            },
            ToolSuggestion {
                command: "tungsten cache status",
                cost: 1,
                reason: "Show cache entry counts (AST / signature / full-output) to confirm whether the project cache is populated",
                relevance: 0.75,
            },
            ToolSuggestion {
                command: "tungsten cache clean --dry-run",
                cost: 1,
                reason: "Preview which .tungsten dirs would be removed before clearing",
                relevance: 0.60,
            },
        ],
    },
];
