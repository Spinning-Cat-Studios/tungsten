//! The `doctor check` leaves outside the `type` and `ir` sub-namespaces, for
//! the `info pipeline` inventory (split from its parent at the size cap).

use super::{CostTier, PipelineEntry};

/// The `doctor check` leaves outside the `type` and `ir` sub-namespaces.
pub const DOCTOR_PROJECT_CHECKS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "doctor check module reexport-completeness",
        "tungsten doctor check module reexport-completeness <file>",
        "Check pub use re-export completeness",
    )
    .with_cost(CostTier::Elaborate)
    // The dual, pointed back at (ADR 29.8.26a AC 5): `name-collisions` already
    // named this one, and a one-way cross-reference is how a reader who arrives
    // at the wrong half of a pair stays there.
    .with_see_also(&["doctor check module name-collisions"]),
    PipelineEntry::subcommand(
        "doctor check module name-collisions",
        "tungsten doctor check module name-collisions <file> [--severity all|live] [--json]",
        "Report value names defined in >1 reachable module. Both compilers key\n\
         values on the BARE NAME, so the walk's last registration wins and the\n\
         loser's call sites report E0016 in THEIR files naming the winner's\n\
         module. Names both modules and marks the winner. Parse-only, so it\n\
         runs on a file the elaborator REJECTS. Advisory: exit 0 even with\n\
         findings (ADR 13.8.26c)",
    )
    .with_cost(CostTier::Parse)
    .with_see_also(&["doctor check module reexport-completeness"]),
    PipelineEntry::subcommand(
        "doctor check codegen tco-coverage",
        "tungsten doctor check codegen tco-coverage <file> [--gate]",
        "Rank self-recursive fns by O(N)-stack risk from the actual musttail\n\
         gate; --gate is the deterministic CI form (make check-tco-gate),\n\
         exit≠0 on an un-allowlisted HIGH-risk SKIP. Self-recursive ONLY:\n\
         non-self tail edges are counted as SKIP_NON_SELF, never ranked or\n\
         gated (ADRs 1.7.26b, 1.7.26e, 5.8.26a)",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen()
    .with_see_also(&["info codegen musttail-eligibility"]),
    PipelineEntry::subcommand(
        "doctor check codegen unit-cost",
        "tungsten doctor check codegen unit-cost <file> [--threshold 0.5s|8GB] [--json]",
        "Ranked per-unit codegen cost census: wall time + alloc volume;\n\
         threshold gates the exit code. --emit-serial-list prints the\n\
         TUNGSTEN_CODEGEN_SERIAL_UNITS value for units at/above it\n\
         (ADR 8.7.26a)",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "doctor check codegen mono-coverage",
        "tungsten doctor check codegen mono-coverage <file>",
        "Verify all TyApp sites have mono owners",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "doctor check codegen extern-map-ambiguity",
        "tungsten doctor check codegen extern-map-ambiguity <file> [--json]",
        "Detect calls silently bound to the wrong same-named def via the\n\
         clobber-last extern map; exit≠0 on findings (ADR 12.7.26b)",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen(),
    // Absent from the hand-written inventory for as long as it has existed —
    // the live drift instance ADR 28.7.26f §1.1 was written around, and the
    // first thing the completeness test catches.
    PipelineEntry::subcommand(
        "doctor check link collisions",
        "tungsten doctor check link collisions <dir>",
        "Run `nm -g` over a directory of .o files and report duplicate defined\n\
         text symbols — the duplicate-symbol link failure, before the linker\n\
         (ADR 6.5.26d §2.5). suggest-tools names this one for 'referenced but\n\
         not declared'",
    )
    .with_cost(CostTier::Instant)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "doctor check module overlap",
        "tungsten doctor check module overlap",
        "Detect foo.rs + foo/mod.rs coexistence (E0761)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check module signature-collection",
        "tungsten doctor check module signature-collection <file>",
        "Check Signature Collection global collection health",
    )
    .with_cost(CostTier::Elaborate)
    // A signature-collection fault is usually a bad import in ANOTHER module,
    // which is where the re-export check looks.
    .with_see_also(&["doctor check module reexport-completeness"]),
    PipelineEntry::subcommand(
        "doctor check link health",
        "tungsten doctor check link health <binary>",
        "Verify compiled binary stack size + executability",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check link extern-symbols",
        "tungsten doctor check link extern-symbols <file>",
        "Report extern-C decls no tungsten_core symbol PROVIDES — each is an\n\
         undefined reference at link time, minutes into a self-compile, after a\n\
         clean check/test/check-health. Two source walks, no build needed;\n\
         --core-root overrides the scanned root. Exit 2 on findings, and on an\n\
         empty corpus, since `0 examined` must not read like `0 findings`",
    )
    .with_cost(CostTier::Parse)
    .with_see_also(&["doctor check extern-coverage"]),
    PipelineEntry::subcommand(
        "doctor check selfhost closed-terms",
        "tungsten doctor check selfhost closed-terms <file>",
        "Self-hosted Core terms with FREE value variables — a name the\n\
         elaborator resolved through its env and never bound. Type-checks and\n\
         compiles, so nothing else sees it. Spawns tungsten1; exit 2 unasked",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&[
        "diff selfhost-core",
        "doctor check type termination",
        "doctor check selfhost well-typed-terms",
    ]),
    PipelineEntry::subcommand(
        "doctor check selfhost well-typed-terms",
        "tungsten doctor check selfhost well-typed-terms <file>",
        "Self-hosted eliminators over the WRONG former — `fst` of a scalar,\n\
         `app` of a lambda's result. Closed, type-checks, compiles, so\n\
         closed-terms passes it. Shrink-only baseline over main.tg (300 of\n\
         2302); spawns tungsten1; exit 2 unasked (ADR 3.9.26h)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["doctor check selfhost closed-terms", "diff selfhost-core"]),
    PipelineEntry::subcommand(
        "doctor check self-compile-readiness",
        "tungsten doctor check self-compile-readiness",
        "Pre-flight checks for self-compile (filesystem, linker, LLVM)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor check nested-patterns",
        "tungsten doctor check nested-patterns <file>",
        "Detect nested constructor+tuple match patterns (ADR 20.5.26a)",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "doctor check sorry-sites",
        "tungsten doctor check sorry-sites <file>",
        "Which definitions carry a proof hole, and who put it there? One row\n\
         per definition whose Core holds a Sorry: authored (file:line:col),\n\
         synthesised (absurd branch / unreachable pattern arm — a nested\n\
         pattern's lowering) and unclassified. The census behind `check`'s\n\
         `contains sorry`, with no codegen; exit 0 (ADR 18.9.26g)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["doctor check nested-patterns", "info def"]),
    PipelineEntry::subcommand(
        "doctor check extern-coverage",
        "tungsten doctor check extern-coverage <file>",
        "Report extern-C decls the evaluator CANNOT execute — their calls go\n\
         silently Stuck on run/test/playground (native codegen unaffected);\n\
         exit 2 on findings (ADR 28.7.26a)",
    )
    .with_cost(CostTier::Parse)
    .with_see_also(&["info eval externs", "doctor check link extern-symbols"]),
    PipelineEntry::subcommand(
        "doctor check comparable",
        "tungsten doctor check comparable <type> <file>",
        "Can the structural comparator handle this type, and where does it break?
         Reports the gate's own verdict (opaque leaf / incomplete closure /
         unsettled synthesis), so it cannot disagree with what a run does.
         Since 1.8.26c a generic instantiation (List<T>) resolves rather than
         being an opaque leaf. --all checks every type in ONE elaboration; clean
         the cache first (record/adt types are uncached, so a cache hit reports
         everything noncomparable). ADRs 1.8.26b/1.8.26c; exit 1 findings, 2 bad input",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["info type size", "info type mu-members"]),
    PipelineEntry::subcommand(
        "doctor map-span",
        "tungsten doctor map-span <file> <offset>",
        "Map byte offset to file:line:col",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "doctor suggest-tools",
        "tungsten doctor suggest-tools <desc>",
        "Suggest diagnostic tools for an error — start here when stuck",
    )
    .with_cost(CostTier::Instant),
];
