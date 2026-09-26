//! The `tungsten doctor` health checks, for the `info pipeline` inventory.
//!
//! `tungsten explain` moved out to [`super::explain_commands`]: it is not a
//! doctor command, and this file had grown past its size threshold carrying
//! both. The `doctor check ir` family lives in [`super::doctor_ir_audits`], and
//! the project checks outside the `type`/`ir` sub-namespaces in
//! [`project_checks`] (split out at the size cap by ADR 18.9.26g).

mod project_checks;

pub use project_checks::DOCTOR_PROJECT_CHECKS;

use super::{CostTier, PipelineEntry};

pub const DOCTOR_COMMANDS: &[PipelineEntry] = &[
    PipelineEntry::subcommand(
        "doctor self-test",
        "tungsten doctor self-test",
        "Smoke test the compiler",
    )
    .with_cost(CostTier::CompileAndRun),
    PipelineEntry::subcommand(
        "doctor audit-recursion",
        "tungsten doctor audit-recursion",
        "Identify and classify recursive functions (consults codegen for the\n\
         actual musttail verdict; --source-only to skip; ADR 1.7.26b)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor audit-mutual-types",
        "tungsten doctor audit-mutual-types",
        "Identify mutually recursive type groups",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor audit-dead-definitions",
        "tungsten doctor audit-dead-definitions <file>",
        "Census the definitions no entry point reaches. Reachability from\n\
         roots (`main`, `test_*`, plus --root), NOT an in-degree count: a\n\
         mutually recursive island has callers and is still dead. PER ENTRY\n\
         FILE — a helper only a test_*.tg suite calls is correctly listed and\n\
         is not deletable. Reports, never gates.\n\
         See also: info def <name> <file> --callers (ADR 12.8.26b retro)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor audit-driver-reach",
        "tungsten doctor audit-driver-reach <file>",
        "Partition MODULES into driver-reached / test-only / unreached, ACROSS\n\
         entry files — the aggregation audit-dead-definitions cannot express.\n\
         Reach is the `use` graph, not the module tree: main.tg declares\n\
         `mod codegen;` and no driver path imports it. Test roots are the\n\
         sibling test_*.tg / mustfail_*.tg files; --test-entry adds more.\n\
         Reports, never gates (ADR 3.9.26a)",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "doctor audit-orphan-sources",
        "tungsten doctor audit-orphan-sources <file>",
        "Census the .tg FILES no module tree declares — the filesystem minus\n\
         the tree, the one direction check / audit-dead-definitions /\n\
         audit-driver-reach cannot walk. Remainder split three ways: stranded,\n\
         build-swapped (a make recipe copies it into place — not debt) and\n\
         entry files. Reports, never gates (ADR 3.9.26q)",
    )
    .with_cost(CostTier::Parse),
    PipelineEntry::subcommand(
        "doctor check type determinism normalization",
        "tungsten doctor check type determinism normalization <file>",
        "Check encoding normalization consistency (single-module:\n\
         standalone re-elaboration; multi-module: per-module oracle —\n\
         source-fresh Phase-1e re-collection per module, ADR 22.7.26b;\n\
         live-elaborator normalization fallback, 21.7.26j). --raw-only\n\
         drops tier-2 normalization (ADR 22.7.26c); tier-1 raw == suffices\n\
         since ADR 22.7.26d — raw-only divergence is a regression, not noise",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type determinism encoding",
        "tungsten doctor check type determinism encoding <file>",
        "Elaborate twice and diff the stored Phase-1e maps with strict == —\n\
         is the encoding byte-stable across runs? --json for two-process\n\
         diffing (ADR 22.7.26c)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type determinism resolution-attempts",
        "tungsten doctor check type determinism resolution-attempts <file>",
        "Elaborate twice and diff the per-target count of deferred\n\
         type-reference resolution attempts — the work-side twin of\n\
         determinism encoding (catches the §6.1 attempt-count flap a results\n\
         comparison misses); doubles as an attempt counter (--verbose);\n\
         --json for two-process diffing (ADR 23.7.26a)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type positivity",
        "tungsten doctor check type positivity <file>",
        "Strict-positivity report over a corpus that COMPILES (ADR 7.8.26e).\n\
         Same engine as the E0061 gate, but it elaborates first — so on a\n\
         file the gate rejects it exits 2 and never reaches its verdict;\n\
         read the message to diagnose one. What it adds: each parameter's\n\
         computed occurrence (unused / strict / forbidden, --verbose), the\n\
         expanded graph's largest SCC and Tarjan depth, and unresolved\n\
         App/Adt heads split by lossy stub vs absent",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["explain error"]),
    PipelineEntry::subcommand(
        "doctor check type vacuous-mu",
        "tungsten doctor check type vacuous-mu <file>",
        "Census of types whose cached encoding collapsed to a vacuous mu\n\
         (ADR 11.8.26c). A NESTED inductive family — recursion under a\n\
         generic parameter, type Rose = Node(Wrap<Rose>) — has nowhere to\n\
         put the occurrence, so it encodes as a binder whose body is the\n\
         binder, and every match on it is rejected E0064. The point is\n\
         TIMING: the definition is accepted and checks clean, so a project\n\
         carries the shape invisibly until the first match is written.\n\
         Reachable where info type type-encoding is not — that command is\n\
         blocked by E0064 on any file already matching on one. Exits\n\
         non-zero naming each type and its binder",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&["explain error", "info type type-encoding"]),
    PipelineEntry::subcommand(
        "doctor check type termination",
        "tungsten doctor check type termination <file>",
        "Phase-1 structural-recursion admission census (ADR 29.6.26e).
         Same engine as the E0062 / E0063 gate, which is HARD since ADR
         11.8.26b. Unlike positivity it IS still reachable on a rejected
         file, because it forces Report enforcement for its own
         elaboration (ADR 12.8.26a); its exit code comes from the census,
         not from enforcement. Reports recursive groups certified,
         definitions admitted opaquely via #[partial] taint, and definitions
         not admitted, each with its decreasing parameter and the offending
         argument. --termination proofs demotes executable rejections",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&[
        "explain error",
        "doctor check type positivity",
        "doctor tool-reachability",
    ]),
    PipelineEntry::subcommand(
        "doctor tool-reachability",
        "tungsten doctor tool-reachability",
        "Does each failure mode's companion diagnostic still reach its own
         verdict? (ADRs 12.8.26a, 13.8.26c) Making a gate hard can silently
         delete the report aimed at the files it rejects, because the report
         elaborates first. Runs each (failure mode, companion) pairing
         against a fixture — a file SET since 13.8.26c, so module-scoped
         failures can be expressed — and fails when reality disagrees with
         what the pairing declares, in EITHER direction. Not only hard
         gates: E0016 is an ordinary elaboration error and aborts just as
         thoroughly",
    )
    .with_cost(CostTier::Elaborate)
    .with_see_also(&[
        "doctor check type termination",
        "doctor check type positivity",
        "doctor check module name-collisions",
    ]),
    PipelineEntry::subcommand(
        "doctor check type encoding-depth",
        "tungsten doctor check type encoding-depth <file>",
        "Check encoding stack / type-term depth",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type type-sizes",
        "tungsten doctor check type type-sizes <file>",
        "Report node counts for all type encodings",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type fold-consistency",
        "tungsten doctor check type fold-consistency <file>",
        "Check fold/unfold consistency for all ADTs",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type integrity type-stubs",
        "tungsten doctor check type integrity type-stubs <file>",
        "Detect residual type stubs after elaboration — type names still\n\
         registered as stubs once the pipeline has finished (ADR 6.5.26a).\n\
         Spelled `stubs` before the grouping; the flat path still works",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type integrity constructor-stubs",
        "tungsten doctor check type integrity constructor-stubs <file>",
        "Detect stale constructor stubs — an encoded type or constructor\n\
         field left as a raw TyVar naming a known ADT, which is what makes\n\
         cross-module match dispatch fail E0999",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type integrity constructor-counts",
        "tungsten doctor check type integrity constructor-counts <file>",
        "Validate constructor-list integrity for all ADTs — entry count,\n\
         unique and contiguous indices, unique names, parent consistency\n\
         (ADR 7.5.26e). The sibling of integrity constructor-stubs",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type integrity phase-invariants",
        "tungsten doctor check type integrity phase-invariants <file>",
        "Run the elaboration pipeline with invariant checks at every phase\n\
         boundary and report the violations (ADR 20.4.26e)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type forall-resolution",
        "tungsten doctor check type forall-resolution <file>",
        "Detect inner foralls in structural positions (ADR 21.5.26b)",
    )
    .with_cost(CostTier::Elaborate),
    PipelineEntry::subcommand(
        "doctor check type lowering-consistency",
        "tungsten doctor check type lowering-consistency <file>",
        "Assert every ADT lowers identically via every route — the\n\
         named-vs-structural split-brain gate (--json; ADR 12.7.26c)",
    )
    .with_cost(CostTier::Compile)
    .requiring_codegen()
    .with_see_also(&["info type lowering"]),
];
