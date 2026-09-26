//! The prose-only blocks of the `info pipeline` inventory (GDB recipes,
//! profiling, cross-file enrichment) plus the classification of every clap leaf
//! this inventory deliberately omits.

use super::PipelineEntry;

/// Rendered verbatim, so the leading spaces below are the output's indentation.
pub const GDB_DEBUGGING: &[PipelineEntry] = &[PipelineEntry::note(
    "  gdb ./tungsten1                      Debug self-compiled binary
  break <function>$direct              Break on a .tg function (use $direct suffix)
  info registers x0 x1 x2 x3           Inspect arguments (aarch64)
  info registers rdi rsi rdx rcx       Inspect arguments (x86_64)",
)];

pub const PROFILING: &[PipelineEntry] = &[
    PipelineEntry::note(
        "  Four DIFFERENT profilers — pick by what you are measuring:
    PHASE, Rust bootstrap        → TUNGSTEN_ELAB_PROFILE=1 (cost 1, start here)
    CPU, self-compiled compiler  → make devcontainer-profile-selfcompiled
    HEAP, self-compiled compiler → tungsten-dev selfcompiled-profile
    Chrome trace, Rust bootstrap → make devcontainer-profile (below)
  NOT `make profile`: it samples the Rust bootstrap, where compiled .tg symbols
  never execute as native code, so their shares would falsely read ~0%.",
    ),
    PipelineEntry::note(
        "  TUNGSTEN_ELAB_PROFILE=1 tungsten check <file>   [cost 1, host-side]
  Per-phase stderr table: Stub Registration / Signature Collection / Body
  Elaboration + Total, then a Body Elaboration breakdown reading
  `N modules, M cache hits, K fresh` split into Collection / Body elab /
  Cache writes, plus the ten slowest modules by name (ADR 11.5.26b P0).
  Reach for it BEFORE perf/samply when the question is `which phase?` — no
  container, no codegen feature, no profiler build. Its cache-hit column is
  the cheapest answer to `was the elaboration cache actually consulted?`.
  Two caveats: percentages are shares of the ELABORATION total, not of
  wall-clock; and it profiles the BOOTSTRAP elaborator, so it says nothing
  about a self-compiled tungsten1 run (same trap as `make profile`).",
    ),
    PipelineEntry::make_target(
        "make devcontainer-profile-selfcompiled",
        "CPU-profile the self-compiled self-check (tungsten1 + perf, in-container).\n\
         Cache-cleans, then `perf record -F 997` over `tungsten1 check main.tg`.\n\
         Needs a fresh -O2 tungsten1 (`tungsten-dev self-compile --opt`) — -O0\n\
         inlining blurs $direct frame attribution. Pair with\n\
         `info codegen symbols --by-function` to sum a function's symbols.",
    ),
    PipelineEntry::note(
        "  Build:  cargo build --release -p tungsten_bootstrap --features codegen,profile
  Run:    TUNGSTEN_TRACE_FILE=<path> ./target/release/tungsten compile <file> -o <out>",
    ),
    PipelineEntry::make_target(
        "make devcontainer-profile",
        "Orchestrate the profiling build + trace capture in the devcontainer",
    ),
    PipelineEntry::note(
        "  Tool:   tungsten-dev profile [--jobs N] [--output <path>]

  Env vars:",
    ),
    PipelineEntry::env_var(
        "TUNGSTEN_TRACE_FILE=<path>",
        "Chrome Trace JSON output path (default: target/trace.json)",
    ),
    PipelineEntry::env_var(
        "TUNGSTEN_CODEGEN_JOBS=<n>",
        "Override parallel codegen job count",
    ),
    PipelineEntry::env_var(
        "TUNGSTEN_CODEGEN_SERIAL_UNITS=<a,b>",
        "Serialize named codegen units onto one worker (generic OOM\n\
         mitigation, normally unset — the ADR 3.7.26b pathological units were\n\
         fixed by ADR 7.7.26k; generate a value with\n\
         `doctor check codegen unit-cost --emit-serial-list`)",
    ),
    PipelineEntry::env_var(
        "TUNGSTEN_MU_UNFOLD_STATS=1",
        "Per-μ-binder-chain unfold stats to stderr during codegen (call counts\n\
         per chain; node counts/depth/wall once per distinct chain;\n\
         ADR 7.7.26k sizing instrument)",
    ),
    PipelineEntry::env_var(
        "TUNGSTEN_TRACE_ENCODING=<type>|all",
        "Live Phase-1d resolution trace on EVERY entry path, including the\n\
         doctor oracle (ADR 22.7.26d)",
    ),
    PipelineEntry::note(
        "
  Per-unit census lines (ADR 8.7.26a): `tungsten compile -v` prints
    [perf] unit <name>: <time>s alloc=<bytes> [i/N]
  per codegen unit — wall time, allocation volume, and the unit's stable
  index of N total (i matches the emitted-filename prefix).

  Output: Chrome Trace Format JSON → open in https://ui.perfetto.dev
  Traces land in .devcontainer/logs/profiles/ on the host (bind mount).",
    ),
];

pub const CROSS_FILE_DIAGNOSTICS: &[PipelineEntry] = &[PipelineEntry::note(
    "  Enriched error types:
    - Argument type mismatch   → cross-file note: 'parameter type declared in `fn`'
    - Return type mismatch     → cross-file note + trace: 'return type declared in `fn`'
  Enrichment requires:
    - Callee is in a different module from the call site
    - Callee's module file is present in the SourceMap
  Limitations:
    - Only direct calls (Expr::App(Expr::Path(...))) are enriched
    - Higher-order calls (let f = get_fn; f()) do not get cross-file notes
    - Note span covers the whole function definition, not just the return type",
)];

/// Clap leaves that are *not* diagnostic tooling, each with the reason stated
/// here rather than in a side file (ADR 28.7.26f D2a).
///
/// This is what makes the completeness check exhaustive by construction: the
/// test fails on any leaf that appears in neither this list nor a documented
/// entry, whereas a plain exclusion list is silently satisfied by its own
/// existence. The section renders as a short closing paragraph so a reader can
/// see the boundary the inventory draws.
pub const NOT_DIAGNOSTIC: &[PipelineEntry] = &[
    PipelineEntry::note(
        "  Classified out of this inventory deliberately — `tungsten commands` lists
  the complete tree:",
    ),
    PipelineEntry::not_diagnostic("check", "core workflow command, not a diagnostic"),
    PipelineEntry::not_diagnostic("run", "core workflow command, not a diagnostic"),
    PipelineEntry::not_diagnostic("expr eval", "core workflow command, not a diagnostic"),
    PipelineEntry::not_diagnostic("expr repl", "core workflow command, not a diagnostic"),
];
