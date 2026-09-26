//! The `tungsten info` namespace, for the `info pipeline` inventory.

use super::{CostTier, PipelineEntry};

pub const INFO_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "info type types",
        "tungsten info type types <file>",
        "List all types",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type adt",
        "tungsten info type adt <name> <file>",
        "Show ADT details (--show-fields, --check-fold)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type members constructors",
        "tungsten info type members constructors <name> <file> [--raw]",
        "Show constructor entries with duplicate detection; --raw prints each\n\
         field's stored Type verbatim (Display strips the Type-Body Collection\n\
         @-prefix, so @List and List are indistinguishable without it —\n\
         ADR 1.8.26c)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type encoding",
        "tungsten info type encoding <name> <file>",
        "Explain encoding strategy",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type type-encoding",
        "tungsten info type type-encoding <name> <file>",
        "Display μ-type encoding tree",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type size",
        "tungsten info type size <name> <file>",
        "Stored-Type-tree size metrics: node count, depth, μ-binder\n\
         chain, α-occurrences per binder (ADR 8.7.26a)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type spine",
        "tungsten info type spine <name> <file>",
        "A record's declared field count beside its encoded product\n\
         spine length. A field type that is structurally a product\n\
         (tuple, alias to one, single-constructor ADT) is SPLICED in;\n\
         a named record or generic instantiation stays a reference —\n\
         so a 3-field record can have a 4-long spine (ADR 7.9.26c)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["info type members record-fields", "info type type-encoding"]),
    PipelineEntry::subcommand(
        "info type mu-members",
        "tungsten info type mu-members <name> <file>",
        "What each μ-binder in a type's chain denotes. A cluster's\n\
         encoding does NOT carry the other members' bodies, so reading\n\
         it as a closed type gives a wrong answer (ADR 1.8.26b)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["doctor check comparable", "info type size"]),
    PipelineEntry::subcommand(
        "info type mutual-recursion-groups",
        "tungsten info type mutual-recursion-groups <file>",
        "Show SCC groups and μ-binder order",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type encode-order",
        "tungsten info type encode-order <file>",
        "Deterministic Phase-1e encode order (why X before Y);\n\
         --focus <type>, --all; flags order-defeating SCCs (ADR 22.7.26d)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type members field-type",
        "tungsten info type members field-type <Type.field> <file>",
        "Show stored + resolved type for a field",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type members record-fields",
        "tungsten info type members record-fields <name> <file>",
        "Show record fields with types and product positions",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type members visibility",
        "tungsten info type members visibility <name> <file>",
        "Show effective visibility of constructors/fields (ADR 14.5.26c)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info type lowering",
        "tungsten info type lowering <name> <file>",
        "Show an ADT's LLVM layout via each lowering route (named/app/\n\
         structural/flat-adt); flags divergence (ADR 12.7.26c)",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen()
    .with_see_also(&["doctor check type lowering-consistency"]),
    PipelineEntry::subcommand(
        "info codegen symbols",
        "tungsten info codegen symbols <file> [--by-function <fn>]",
        "Lambda → source name mapping; --by-function lists the FULL symbol set \
         one .tg function compiles to (`<name>`, `$direct`, `$direct_mt`, each \
         `_lambda_N`) with each one's role — the set a perf profile's self time \
         must be summed across, since `<name>` + `$direct` alone under-counts \
         (ADR 5.8.26b: 56.94% of that profile sat in `$direct_mt`)",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "info codegen abi",
        "tungsten info codegen abi <fn> <file.ll>",
        "Inspect ABI layout and passing decisions",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "info codegen units",
        "tungsten info codegen units <file>",
        "Show per-function unit partitioning",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "info codegen unit-paths",
        "tungsten info codegen unit-paths <file> [-o <dir>] [--json]",
        "Where each unit's .ll lands + which units COLLIDE. Two units whose\n\
         paths differ only in case overwrite each other on APFS/NTFS, and the\n\
         loser is a unit no `doctor check ir` audit ever sees (the compiler\n\
         emits 2055 units into 2049 files). Exits non-zero on any collision;\n\
         shares the emitter's own destination rule (ADR 28.7.26e)",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "info codegen mono",
        "tungsten info codegen mono <file>",
        "Show mono ownership table",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "info codegen musttail-eligibility",
        "tungsten info codegen musttail-eligibility <fn> <file>",
        "Drill into one function's musttail blockers (ADR 1.7.26b)",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen()
    .with_see_also(&["doctor check codegen tco-coverage"]),
    PipelineEntry::subcommand(
        "info codegen indirect-abi",
        "tungsten info codegen indirect-abi <fn> <file>",
        "Per-param Class-P lowering (by-value/decomposed/indirect) + slot\n\
         layout + per-slot ABI attributes incl. noalias\n\
         (ADRs 1.7.26e, 17.7.26e)",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen()
    .with_see_also(&["doctor check ir indirect-buffers"]),
    PipelineEntry::subcommand(
        "info cir sites",
        "tungsten info cir sites <variant> <file>",
        "Find CIR constructor application sites",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "info cir constructors",
        "tungsten info cir constructors <file>",
        "List all CodegenIR constructors with arities",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "info eval trace",
        "tungsten info eval trace <def> <file>",
        "Trace the evaluator step-by-step: step index, bounded node\n\
         count, depth-limited shape; --max-steps/--shape-depth/\n\
         --limit-nodes (ADR 21.7.26j)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info eval externs",
        "tungsten info eval externs",
        "List the tg_* externs the evaluator executes; every OTHER\n\
         ExternCall goes silently Stuck (no error, no output). --json\n\
         (ADR 28.7.26a)",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&[
        "doctor check extern-coverage",
        "info eval reachable-externs",
    ]),
    PipelineEntry::subcommand(
        "info eval reachable-externs",
        "tungsten info eval reachable-externs <def> <file> [--defs a,b] [--max-visited N]",
        "Which externs a definition's call path reaches, each with\n\
         the shortest call chain and whether the evaluator can run it.\n\
         Unlike doctor check extern-coverage (declaration scope, reads\n\
         the same for a sound file and a vacuous one) this answers\n\
         'can I unit-test this, or will the assertion go silently\n\
         Stuck?'. STATIC reachability: every arm a callee COULD take,\n\
         not the ones your input triggers. When NOTHING blocks it also\n\
         flags the inverse — assertable, yet no test_* in this entry\n\
         file calls it, so cost-5 coverage is going unused. --defs adds\n\
         roots answered from ONE elaboration (~99% of the cost), one\n\
         section each in request order; --max-visited bounds each walk\n\
         and reports INCOMPLETE naming what went unread. --json\n\
         carries a blocking count, complete, not_reached,\n\
         reached_by_tests and assertable_but_untested — an array for a\n\
         set of roots (ADR 7.8.26a, 7.8.26c, 19.8.26c, 3.9.26c)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["info eval externs", "doctor check extern-coverage"]),
    PipelineEntry::subcommand(
        "info module tree",
        "tungsten info module tree <file>",
        "Show module hierarchy + elaboration order",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "info module imports",
        "tungsten info module imports <module> <file>",
        "Show import resolution status (stub vs full def)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info module reexport-chain",
        "tungsten info module reexport-chain <module> <file>",
        "Trace re-export paths for a module's items",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info module dependents",
        "tungsten info module dependents <module> <file>",
        "Who depends on a module, and by which path — the inverse of\n\
         info module imports. DIRECT dependents name the module in\n\
         their path and break when it moves; INDIRECT ones reach the\n\
         same items through a re-export and do not, and grep cannot\n\
         tell the two apart. A third section lists .tg string literals\n\
         holding the module path — real dependencies no resolved table\n\
         can see, found by text and labelled as such. --verbose lists\n\
         the indirect sites rather than counting them per hop. The\n\
         question every directory regroup asks (ADR 5.9.26f)",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["info module imports", "info module reexport-chain"]),
    PipelineEntry::subcommand(
        "info module alias-table",
        "tungsten info module alias-table <module> <file>",
        "Show import alias mappings (alias ← original)",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "info module import-targets",
        "tungsten info module import-targets <module> <file>",
        "Show the value-import-target table codegen uses for colliding\n\
         names (ADR 12.7.26a)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info def",
        "tungsten info def <name> <file>",
        "Show definition type + Core IR. --why-not-certified adds the
         termination view: per parameter, whether it is a candidate
         decreasing root and which class of type refused it (ADR 12.8.26a).
         The rendered type alone cannot answer that — Display prints a
         mutual-cluster marker and a real ADT identically",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["doctor check type termination", "explain error"]),
    PipelineEntry::subcommand(
        "info try-desugar",
        "tungsten info try-desugar <name> <file>",
        "Show `?` operator desugaring in a definition",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info error-enrichment",
        "tungsten info error-enrichment <file>",
        "Show cross-file diagnostic enrichment points (ADR 15.5.26a)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "info error-sites",
        "tungsten info error-sites <code>",
        "Where an error code is raised in the compiler, by enclosing function",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["explain error", "doctor suggest-tools"]),
    PipelineEntry::subcommand(
        "info builtins",
        "tungsten info builtins [<name>]",
        "Which bare names each compiler intercepts BEFORE name resolution,\n\
         with every asymmetry marked (ADR 20.8.26c)",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["info def", "doctor audit-dead-definitions"]),
    PipelineEntry::subcommand(
        "info pipeline",
        "tungsten info pipeline [--json]",
        "This message",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["commands"]),
    PipelineEntry::subcommand(
        "commands",
        "tungsten commands [--tree] [--json]",
        "Flat/tree/JSON listing of every non-hidden subcommand, generated\n\
         from the clap tree — the structural half of this inventory",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["info pipeline"]),
];
