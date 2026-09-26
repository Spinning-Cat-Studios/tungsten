//! Runtime / codegen / self-host error patterns: crashes, hangs, ABI and
//! linker issues, miscompiles, and bootstrap-vs-self-host divergence.
//!
//! The retired level labels `l1`/`l2`/`l3` (ADR 24.7.26e) survive in the
//! `keywords` arrays as LEGACY SEARCH SYNONYMS only — they are matcher inputs,
//! so a user typing the old vocabulary still gets a hit. `l4`/`l5` are the
//! ADR 9.2.26 verification ladder (the out-of-scope generation axis) and keep
//! their spelling. No naming *surface* here uses a level label.
//!
//! **Keywords must be lowercase.** `scored_patterns` lowercases the user's
//! description and then tests `desc_lower.contains(keyword)`, so a keyword
//! carrying an uppercase letter can never match — it is dead weight that reads
//! like a working synonym. `keywords_are_lowercase` in `../tests.rs` enforces
//! this across every pattern table.
//!
//! **Two vocabularies, not one** (ADR 4.9.26d). Each table below carries the
//! words of the *message* and the words of the *observation*, because the reader
//! the mandated first step exists for has only the second: they saw a binary die
//! and a field come back wrong, and cannot yet say "fold/unfold mismatch" or
//! "self-host divergence". The symptom triggers are marked where they are added,
//! and `symptom_reachability_tests` gates one phrasing per pattern.

use super::{ErrorPattern, ToolSuggestion};

pub(in crate::doctor::suggest_tools::patterns) const RUNTIME_PATTERNS: &[ErrorPattern] = &[
    // ── Segfault / SIGSEGV ──────────────────────────────────────────
    ErrorPattern {
        category: "segfault",
        keywords: &[
            "sigsegv", "segfault", "segmentation fault", "signal 11",
            "null pointer", "access violation",
            // Symptom (4.9.26d): the binary died and the user has no signal
            // name — only that it stopped, immediately, with nothing printed.
            "crash", "dies", "died", "abort", "core dump",
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
    // ── Stack overflow ──────────────────────────────────────────────
    ErrorPattern {
        category: "stack overflow",
        keywords: &[
            "stack overflow", "stack exhaustion", "thread.*overflowed",
            "deep recursion", "recursion limit",
            // Symptom (4.9.26d): the size-dependence IS the observation —
            // fine on a small input, dead on a big one.
            "large input", "big input", "long list", "recursing",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check link health <binary>",
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
    // Scoped to the compiled PROGRAM looping. The COMPILER wedging is the
    // "compiler wedge" pattern below — a different bug with disjoint tools,
    // kept separate rather than merged because audit-recursion *elaborates*,
    // so on the input that wedged the compiler it wedges too.
    ErrorPattern {
        category: "infinite loop",
        keywords: &[
            "infinite loop", "hang", "not terminating", "stuck",
            "never returns", "timeout", "timed out",
            // Symptom (4.9.26d): the compiled PROGRAM, not the compiler —
            // it started, it prints nothing, and it is still going.
            "runs forever", "loops forever", "never comes back",
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
    // ── Compiler wedge (the compiler itself produces no output) ──────
    // ADR 29.6.26e's retrospective: `tungsten check` never returned on a
    // nested inductive family, and the only tool that could say why was
    // `sample`. Every elaborating diagnostic hangs on the same input, so the
    // "infinite loop" suggestions above are actively misleading here.
    //
    // ADR 11.8.26c then diagnosed that hang and corrected this pattern's own
    // reasoning. Two things changed. The cause is now KNOWN — a vacuous
    // `μX. X` encoding — and has a cheap check of its own, so the route no
    // longer dead-ends at `sample`. And the old "constant RSS with no output
    // is a spin" was the right conclusion from the wrong evidence: RSS is
    // silent about which it is. `ps -o time` across TWO samples is the tell,
    // and that bug measured 99.7% CPU with time advancing at flat RSS.
    ErrorPattern {
        category: "compiler wedge",
        keywords: &[
            "compiler hangs",
            "check hangs",
            "compile hangs",
            "wedged",
            "no output",
            "constant rss",
            "spinning",
            "never finishes elaborating",
            "hangs during elaboration",
            "body elaboration",
            "check never returns",
            "compiler spins",
            "compiler loops",
            // Symptom (4.9.26d): what the user actually has is a terminal that
            // has shown nothing for minutes. They cannot yet say "wedged".
            "printed nothing",
            "never finishes",
            "still running after",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "sample <pid>   # and `ps -o %cpu,time,state -p <pid>` TWICE",
                cost: 1,
                reason: "Names the live frame — run it BEFORE killing anything. Then take two `ps -o time` samples: ADVANCING at ~100% cpu is a spin, FROZEN at 0.0 in state UE is a wedge. RSS is flat either way and settles nothing",
                relevance: 0.95,
            },
            ToolSuggestion {
                command: "tungsten doctor check type vacuous-mu <file>",
                cost: 3,
                reason: "The known cause of an elaboration spin: a nested inductive family encodes to `μX. X`, which no unfold can flatten (ADR 11.8.26c). Cheap, and rules the class in or out",
                relevance: 0.88,
            },
            ToolSuggestion {
                command: "tungsten check <file> -v",
                cost: 3,
                reason: "The last phase line printed brackets the wedge (e.g. stops after 'Elaborating module …' = Body Elaboration)",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten cache clean",
                cost: 1,
                reason: "Rule out a stale cache before blaming the input — a poisoned entry can wedge a run that a clean tree completes",
                relevance: 0.60,
            },
        ],
    },
    // --- ABI mismatch ---
    ErrorPattern {
        category: "abi mismatch",
        keywords: &[
            "abi", "layout", "emitter", "abi mismatch",
            // Symptom (4.9.26d): a value crossing the boundary comes back
            // rearranged. The word "ABI" is the diagnosis, not the observation.
            "extern", "wrong order", "calling convention",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten diff abi <type> <file>",
                cost: 3,
                reason: "Compare ABI layout between bootstrap codegen and .tg emitter",
                relevance: 0.95,
            },
        ],
    },
    // ── Nested pattern / unknown value in match ─────────────────────
    ErrorPattern {
        category: "nested pattern",
        keywords: &[
            "unknown value", "nested pattern", "tuple pattern",
            "ok((", "err((", "some((", "constructor tuple",
            "nested match", "nested constructor", "pattern binding",
            // Symptom (4.9.26d): what is observed is an arm variable holding
            // the wrong thing — the pattern shape is the diagnosis that follows.
            "destructuring", "wrong element", "variables bound", "arm variable",
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
                command: "tungsten info type members constructors <name> <file>",
                cost: 3,
                reason: "Verify constructor index (source-order: 0=left/inl, 1=right/inr)",
                relevance: 0.80,
            },
        ],
    },
    // ── Miscompile / wrong runtime value (ADR 3.7.26d §2.3) ────────
    ErrorPattern {
        category: "miscompile / wrong value",
        keywords: &[
            "wrong value", "garbage", "miscompile", "evaluator differs",
            "incorrect output", "prints wrong",
            // Symptom (4.9.26d): the defining observation is that NOTHING went
            // wrong — no error, no crash, just an answer that is not right.
            "wrong answer", "answer is wrong", "wrong result", "evaluator",
            "no error but", "nothing errors",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten diff exec <file>",
                cost: 5,
                reason: "Run the evaluator AND the compiled binary on the same program and compare outputs — the general silent-miscompile detector",
                relevance: 0.98,
            },
            ToolSuggestion {
                command: "tungsten doctor check ir sret-stores <ll-dir>",
                cost: 4,
                reason: "Static lint for sret returns that discard their value (the ADR 3.7.26a defect-2 shape)",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten compile --dump-ir=<fn> <file>",
                cost: 4,
                reason: "Inspect Core IR for the diverging function",
                relevance: 0.80,
            },
            ToolSuggestion {
                command: "tungsten compile --emit-llvm <file>",
                cost: 4,
                reason: "Emit per-function LLVM IR to inspect the lowering",
                relevance: 0.75,
            },
            ToolSuggestion {
                command: "tungsten doctor check extern-map-ambiguity <file>",
                cost: 4,
                reason: "Detect calls silently bound to the wrong same-named def via the clobber-last extern map (ADR 12.7.26a miscompile class)",
                relevance: 0.70,
            },
        ],
    },
    // ── Merge-arms lowering-route divergence (ADR 12.7.26c) ─────────
    // The T2 hard error ("merge arms disagree … two reachable arms lowered to
    // different types") means one Tungsten type took two lowering routes. Its
    // own wording scored zero before this ADR (Problem §3).
    ErrorPattern {
        category: "merge-arms lowering divergence",
        keywords: &[
            "merge arms disagree", "lowered to different types", "reachable arms",
            "lowering route", "named-vs-structural", "split-brain", "route diverge",
            // Symptom (4.9.26d): "the two branches look the same to me and the
            // compiler says otherwise" — the observation behind the T2 error.
            "arms disagree", "two branches", "both branches", "compiles differently",
        ],
        suggestions: &[
            ToolSuggestion {
                command: "tungsten doctor check type lowering-consistency <file>",
                cost: 4,
                reason: "Prove no ADT lowers differently by spelling — the regression gate for the named-vs-structural split-brain (ADR 12.7.26c)",
                relevance: 0.97,
            },
            ToolSuggestion {
                command: "tungsten info type lowering <name> <file>",
                cost: 4,
                reason: "Show one type's LLVM layout via each lowering route (named / app / structural / flat-adt) and flag divergence",
                relevance: 0.90,
            },
            ToolSuggestion {
                command: "tungsten diff ir <a> <b>",
                cost: 4,
                reason: "Structural IR comparison to localize where the two arm layouts diverge",
                relevance: 0.78,
            },
            ToolSuggestion {
                command: "tungsten diff exec <file>",
                cost: 5,
                reason: "Confirm the divergence is (or isn't) an observable miscompile — evaluator vs native",
                relevance: 0.72,
            },
        ],
    },
];
