//! The sidecar, comparison, cache, and test-runner sections of the
//! `info pipeline` inventory.

use super::{CostTier, PipelineEntry};

pub const SIDECAR_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::note(
        "  (OFF by default — opt in via tungsten_sidecar_enabled in .claude/hooks/config.toml,
   or env TUNGSTEN_SIDECAR_ENABLED=1; when off, record/report/start no-op gracefully — ADR 23.7.26e)",
    ),
    PipelineEntry::subcommand(
        "sidecar record-session",
        "tungsten sidecar record-session --error <desc>",
        "Record a debugging session, return session ID",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar report-outcome",
        "tungsten sidecar report-outcome --session <id> <cmd> ok|no",
        "Report whether a diagnostic command helped",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar stats",
        "tungsten sidecar stats",
        "Show session count, pattern count, top commands",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar reset",
        "tungsten sidecar reset",
        "Clear all stored experience data",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar export",
        "tungsten sidecar export --json",
        "Dump full store contents as JSON",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar start",
        "tungsten sidecar start [--repo-root <path>]",
        "Start background sidecar process (Unix domain socket)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "sidecar stop",
        "tungsten sidecar stop",
        "Stop the running sidecar process",
    )
    .with_cost(CostTier::Instant),
];

pub const COMPARISON_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "diff ir",
        "tungsten diff ir <a.ll> <b.ll>",
        "Structural IR diff",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "diff core",
        "tungsten diff core <a> <b>",
        "Structural Core IR diff",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "diff types",
        "tungsten diff types <a> <b> <file>",
        "Structural tree-diff of two type encodings",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "diff abi",
        "tungsten diff abi <type> <file>",
        "Compare ABI layout: bootstrap vs .tg emitter",
    )
    .with_cost(CostTier::Elaborate)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "diff bootstrap-selfhost-check",
        "tungsten diff bootstrap-selfhost-check <file>",
        "Compare bootstrap vs tungsten1 check results",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "diff selfhost-core",
        "tungsten diff selfhost-core <def> <file>",
        "Compare ONE definition's Core term across both compilers —\n\
         the divergence `bootstrap-selfhost-check` cannot see, because\n\
         it compares verdicts rather than terms (ADR 19.8.26d). Read BOTH\n\
         terms: literal and mu-binder spellings differ cosmetically, so the\n\
         first textual difference is often not the structural one",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "diff exec",
        "tungsten diff exec <file>",
        "Run evaluator AND native binary, compare outputs — the\n\
         silent-miscompile detector (ADR 3.7.26d)",
    )
    .with_cost(CostTier::CompileAndRun)
    .requiring_codegen(),
    PipelineEntry::subcommand(
        "diff cache",
        "tungsten diff cache <file> [--gate]",
        "Run cold then warm, compare outputs — the cache-poisoning canary;\n\
         --gate is the CI form, non-zero on a cold-vs-warm divergence\n\
         (ADR 4.7.26d)",
    )
    .with_cost(CostTier::CompileAndRun)
    .with_see_also(&["cache inspect"]),
];

pub const CACHE_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "cache status",
        "tungsten cache status [<file>]",
        "Aggregate AST / signature / full-output entry counts, and the ROOT\n\
         they were counted in — pass the entry file, or the root is the cwd\n\
         and not where the build wrote (ADR 5.8.26d D5)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "cache stats",
        "tungsten cache stats [<file>] [--json]",
        "AST and elaboration entry counts and on-disk sizes, against the\n\
         configured max — sizing, not hit rates; reports its root",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "cache inspect",
        "tungsten cache inspect <file> [--json]",
        "Per-module cache tier + run/test body hazard: a signature-only entry\n\
         reports bodies? = NO (ADR 4.7.26c)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "cache prune",
        "tungsten cache prune [--target-mb <n>]",
        "Evict least-recently-used entries down to a target size (defaults to\n\
         the configured max_size_mb)",
    )
    .with_cost(CostTier::Instant),
    PipelineEntry::subcommand(
        "cache clean",
        "tungsten cache clean [--dry-run]",
        "Remove all .tungsten caches — the remedy for a stale-cache hazard,\n\
         and mandatory after an AST-variant change",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["cache clean-project"]),
    PipelineEntry::subcommand(
        "cache clean-project",
        "tungsten cache clean-project [<file>]",
        "Clear ONE project's cache, resolved from <file>'s parent or the cwd\n\
         (was `tungsten clean`, ADR 19.8.26a); `cache clean` is the broad one",
    )
    .with_cost(CostTier::Instant)
    .with_see_also(&["cache clean"]),
];

pub const TEST_RUNNER: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "test",
        "tungsten test <file>",
        "Discover and run test_* functions",
    )
    .with_cost(CostTier::CompileAndRun),
    PipelineEntry::note(
        "  tungsten test <file> --filter <pat>  Filter tests by name substring
  tungsten test <file> --check-only    Run expect_type only (cost 3, no codegen). Usually
                                       unnecessary: tg-test-tiers.toml declares each entry
                                       file's cost tier and the runner honours it with no
                                       flag, so a tier-3 file is check-only everywhere
                                       (ADR 6.8.26c)
  tungsten test <file> --require-tests Fail on zero discovered tests — no vacuous green (ADR 2.7.26b)
  tungsten test <file> --watchdog <s>  Per-test wall-clock bound; a non-terminating body is
                                       reported TIMEOUT instead of hanging. Default 60s;
                                       0 disables (ADR 21.7.26f)
  tungsten test <file> --assertion-census
                                       Per-test count of assertions actually EXECUTED. A test
                                       that executes ZERO already fails the run on its own
                                       (ASSERTED NOTHING); the census is for the non-zero rows,
                                       where 1-of-3 passes and is two thirds imaginary
                                       (ADR 6.8.26b)",
    ),
];
