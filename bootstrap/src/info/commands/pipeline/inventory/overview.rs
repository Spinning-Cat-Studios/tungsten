//! The pipeline diagram and the `tungsten compile` diagnostic flags.

use super::{CostTier, PipelineEntry};

/// The stage banner. Rendered verbatim, so its leading spaces are output
/// indentation rather than source formatting.
pub const PIPELINE_BANNER: &[PipelineEntry] = &[PipelineEntry::note(
    "Tungsten Compiler Pipeline
══════════════════════════

┌──────────┐   ┌───────────┐   ┌───────────┐   ┌──────────┐   ┌──────────┐
│  Parse   │──▶│ Elaborate  │──▶│ Post-elab │──▶│ Codegen  │──▶│  Link    │
│  (.tg →  │   │ (bidir.   │   │ cleanup   │   │ (LLVM IR │   │ (cc →    │
│  AST)    │   │  types)   │   │ (TyVar    │   │  → .o)   │   │  exe)    │
│          │   │           │   │  subst)   │   │          │   │          │
└──────────┘   └───────────┘   └───────────┘   └──────────┘   └──────────┘

Key types at each boundary:
  Parse output:     Vec<ast::Item>  (surface syntax)
  Elaborate output: Vec<CoreDef>    (typed Core IR terms)
  Codegen input:    Vec<CoreDef>    (cleaned — 0 free TyVars)
  Codegen output:   LLVM Module     (.ll or .o file)",
)];

/// The `compile` leaf itself, kept out of [`COMPILE_FLAGS`] so the flag table's
/// left-hand stage column stays a stage column.
pub const COMPILE_COMMAND: &[PipelineEntry] = &[PipelineEntry::subcommand(
    "compile",
    "tungsten compile <file> [-o <out>]",
    "Compile to an object file or executable; the flags below refine what it emits",
)
.with_cost(CostTier::Compile)
.requiring_codegen()];

pub const COMPILE_FLAGS: &[PipelineEntry] = &[
    PipelineEntry::compile_flag(
        "--trace-types=<name>",
        "Trace type transformations for a definition",
    )
    .in_flag_group("Elaborate:"),
    PipelineEntry::compile_flag(
        "--trace-encoding[=name]",
        "Trace type encoding decisions (stack, cycles, μ-vars)\n\
         (or env TUNGSTEN_TRACE_ENCODING=<type>|all — reaches\n\
         EVERY entry path incl. the doctor oracle, ADR 22.7.26d)",
    ),
    PipelineEntry::compile_flag(
        "--trace-normalization[=name]",
        "Trace normalization path for a type",
    ),
    PipelineEntry::compile_flag(
        "--trace-constructor-registration",
        "Trace constructor registration across phases",
    ),
    PipelineEntry::compile_flag("--dump-types", "Show all type definitions"),
    PipelineEntry::compile_flag(
        "--check-tyvar-escape",
        "Detect free TyVars in monomorphic defs",
    )
    .in_flag_group("Post-elab:"),
    PipelineEntry::compile_flag(
        "--no-codegen",
        "Stop after Core IR; skip LLVM codegen + linking",
    )
    .in_flag_group("Pipeline:"),
    PipelineEntry::compile_flag(
        "--codegen-backtrace",
        "Trace TyVar fallthrough in lower_type",
    )
    .in_flag_group("Codegen:"),
    PipelineEntry::compile_flag("--emit-llvm", "Dump full LLVM IR to file"),
    PipelineEntry::compile_flag("--dump-ir=<name>", "Pretty-print Core IR for a definition"),
    PipelineEntry::compile_flag(
        "--dump-encoding=<name>",
        "Show encoding breakdown for an ADT",
    ),
    PipelineEntry::compile_flag(
        "--debug-info",
        "Emit DWARF debug info (definition-level line tables)",
    ),
    PipelineEntry::compile_flag(
        "--sanitize",
        "Enable AddressSanitizer (links with -fsanitize=address)",
    ),
    PipelineEntry::compile_flag(
        "--trace-adt-ops[=type]",
        "Runtime ADT construct/match tracing",
    ),
    PipelineEntry::compile_flag(
        "--trace-musttail",
        "Trace musttail TCO decisions (ADR 8.5.26c)",
    ),
    PipelineEntry::compile_flag(
        "--trace-escape",
        "Trace escape analysis decisions (ADR 8.5.26d)",
    ),
    PipelineEntry::compile_flag(
        "--trace-mono",
        "Trace monomorphization pipeline (ADR 8.5.26g)",
    ),
    PipelineEntry::compile_flag(
        "--named-lambdas",
        "Emit source-level names for IR functions",
    ),
    PipelineEntry::compile_flag(
        "--alloc-profile[=fn]",
        "Per-function + per-class allocation profiling (ADR 7.5.26b, 2.7.26a)\n\
         Classes: mu_alloc/env_alloc/ref_new/string; interim dump every\n\
         TUNGSTEN_ALLOC_PROFILE_INTERVAL_MB (default 1024, 0=off).\n\
         Each phase/module marker also emits an [arena] line: tungsten_core\n\
         FFI-arena retention (types/terms counts + deep bytes, slab, VmRSS)\n\
         — the Rust-allocator side the classes above cannot see (2.7.26a §3.4).\n\
         Deltas belong to the module named in the EARLIER marker (markers\n\
         fire at module start). Every marker and the final report end with\n\
         the bump-arena tail bump=off|on chunks= reserved= used= hw=\n\
         (ADR 14.9.26b). The flag changes the emitted allocation SHAPE\n\
         (ADR 18.9.26c): profiled, every site calls __tungsten_alloc\n\
         unconditionally; unprofiled, each site branches on the arena mode\n\
         and calls malloc directly when it is off. TUNGSTEN_ARENA=bump[:mib]\n\
         at RUN time selects the arena. One-command self-compiled\n\
         run + per-module summary: tungsten-dev selfcompiled-profile\n\
         (ADR 11.7.26a)",
    ),
    PipelineEntry::compile_flag(
        "--only-unit=<unit>",
        "Compile ONLY the named codegen unit(s) (repeatable; requires\n\
         --emit-llvm, skips __mono depot) — isolate one unit for\n\
         memory/time profiling (ADR 3.7.26b)",
    ),
    PipelineEntry::compile_flag(
        "--dump-synthesized[=sym]",
        "Print synthesized comparator Core terms as they are emitted\n\
         (invisible to --dump-ir; codegen-time intercept). Optional\n\
         symbol-substring filter (ADR 12.7.26c P6)",
    ),
    // `--dump-abi` was listed here as a compile flag marked "(planned)". It is
    // not one, and never was: ABI inspection is the visible `info codegen abi`
    // subcommand (plus a hidden `dump-abi` legacy alias). ADR 28.7.26f's D3
    // flag reconciliation is what surfaced it — the listing had been
    // advertising a flag that errors out.
];

pub const GLOBAL_FLAGS: &[PipelineEntry] = &[
    PipelineEntry::global_flag(
        "--hints",
        "Force diagnostic hints on (default: auto-detect TTY)",
    ),
    PipelineEntry::global_flag("--no-hints", "Suppress diagnostic hints"),
    // Belongs to `check`, not to the root and not to `compile` — the mistyping
    // D3's reconciliation exists to catch.
    PipelineEntry::flag_on(
        "check",
        "--json (check only)",
        "Emit JSON diagnostic report with hints",
    ),
];
