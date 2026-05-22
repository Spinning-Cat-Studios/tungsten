//! `tungsten info pipeline` command implementation.

use std::process::ExitCode;

// ═══════════════════════════════════════════════════════════════════════
// tungsten info pipeline
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_info_pipeline() -> ExitCode {
    print_pipeline_header();
    print_compile_flags();
    print_info_commands();
    print_explain_commands();
    print_doctor_commands();
    print_gdb_debugging();
    print_sidecar_commands();
    print_comparison_commands();
    print_test_command();
    print_profiling();
    print_cross_file_diagnostics();
    println!("\nCost scale: 1=instant  2=parse  3=elaborate  4=compile  5=compile+run\nLower is faster. Start at cost 1; escalate as needed.");
    ExitCode::SUCCESS
}

fn print_pipeline_header() {
    println!(
        "\
Tungsten Compiler Pipeline
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
  Codegen output:   LLVM Module     (.ll or .o file)"
    );
}

fn print_compile_flags() {
    println!(
        "\
\nDiagnostic flags (on `tungsten compile`):
  Elaborate:  --trace-types=<name>    Trace type transformations for a definition
              --trace-encoding[=name] Trace type encoding decisions (stack, cycles, μ-vars)
              --trace-normalization[=name]
                                      Trace normalization path for a type
              --trace-constructor-registration
                                      Trace constructor registration across phases
              --dump-types            Show all type definitions
  Post-elab:  --check-tyvar-escape    Detect free TyVars in monomorphic defs
  Pipeline:   --no-codegen            Stop after Core IR; skip LLVM codegen + linking
  Codegen:    --codegen-backtrace     Trace TyVar fallthrough in lower_type
              --emit-llvm             Dump full LLVM IR to file
              --dump-ir=<name>        Pretty-print Core IR for a definition
              --dump-encoding=<name>  Show encoding breakdown for an ADT
              --debug-info            Emit DWARF debug info (definition-level line tables)
              --sanitize              Enable AddressSanitizer (links with -fsanitize=address)
              --trace-adt-ops[=type]  Runtime ADT construct/match tracing
              --trace-musttail        Trace musttail TCO decisions (ADR 8.5.26c)
              --trace-escape          Trace escape analysis decisions (ADR 8.5.26d)
              --trace-mono            Trace monomorphization pipeline (ADR 8.5.26g)
              --named-lambdas         Emit source-level names for IR functions
              --alloc-profile[=fn]    Per-function allocation profiling (ADR 7.5.26b)
              --dump-abi=<fn>         Show ABI register assignment (planned)

Global flags:
  --hints                 Force diagnostic hints on (default: auto-detect TTY)
  --no-hints              Suppress diagnostic hints
  --json (check only)     Emit JSON diagnostic report with hints"
    );
}

fn print_info_commands() {
    println!(
        "\
\nInfo commands [cost 3 — parse + elaborate]:
  tungsten info type types <file>      List all types
  tungsten info type adt <name> <file> Show ADT details (--show-fields, --check-fold)
  tungsten info type constructors <name> <file>
                                       Show constructor entries with duplicate detection
  tungsten info type encoding <name> <file>
                                       Explain encoding strategy
  tungsten info type type-encoding <name> <file>
                                       Display μ-type encoding tree
  tungsten info type mutual-recursion-groups <file>
                                       Show SCC groups and μ-binder order
  tungsten info type field-type <Type.field> <file>
                                       Show stored + resolved type for a field
  tungsten info type record-fields <name> <file>
                                       Show record fields with types and product positions
  tungsten info type visibility <name> <file>
                                       Show effective visibility of constructors/fields (ADR 14.5.26c)
  tungsten info codegen symbols <file> Show lambda → source name mapping  [requires codegen]
  tungsten info codegen abi <fn> <file.ll>
                                       Inspect ABI layout and passing decisions  [requires codegen]
  tungsten info codegen units <file>   Show per-function unit partitioning  [requires codegen]
  tungsten info codegen mono <file>    Show mono ownership table  [requires codegen]
  tungsten info cir sites <variant> <file>
                                       Find CIR constructor application sites  [cost 2]
  tungsten info cir constructors <file>
                                       List all CodegenIR constructors with arities  [cost 2]
  tungsten info module tree <file>     Show module hierarchy + elaboration order  [cost ≤2]
  tungsten info module imports <module> <file>
                                       Show import resolution status (stub vs full def)
  tungsten info module reexport-chain <module> <file>
                                       Trace re-export paths for a module's items
  tungsten info module alias-table <module> <file>
                                       Show import alias mappings (alias ← original)  [cost ≤2]
  tungsten info def <name> <file>      Show definition type + Core IR
  tungsten info try-desugar <name> <file>
                                       Show `?` operator desugaring in a definition
  tungsten info error-enrichment <file>
                                       Show cross-file diagnostic enrichment points (ADR 15.5.26a)
  tungsten info pipeline               This message  [cost 1]"
    );
}

fn print_explain_commands() {
    println!(
        "\
\nExplain commands [cost 1 — instant, no file I/O]:
  tungsten explain error [<kind>]      Explain an elaboration error (L1)
  tungsten explain error --l2 [<code>] Explain an L2 (self-hosted) error code
  tungsten explain type <string>       Decode a structural Core IR type
  tungsten explain recursion-types     Classification of recursion patterns
  tungsten explain stack-overflow      Understanding stack overflow crashes
  tungsten explain mutual-recursion    Understanding mutual type recursion"
    );
}

fn print_doctor_commands() {
    println!(
        "\
\nHealth check commands [cost 3 — parse + elaborate]:
  tungsten doctor self-test            Smoke test the compiler  [cost 5]
  tungsten doctor audit-recursion      Identify and classify recursive functions
  tungsten doctor audit-mutual-types   Identify mutually recursive type groups
  tungsten diff types <a> <b> <file>
                                       Structural tree-diff of two type encodings
  tungsten doctor check type normalization-consistency <file>
                                       Check encoding normalization consistency
  tungsten doctor check type encoding-depth <file>
                                       Check encoding stack / type-term depth
  tungsten doctor check type type-sizes <file>
                                       Report node counts for all type encodings
  tungsten doctor check type phase-invariants <file>
                                       Validate elaboration phase invariants
  tungsten doctor check type fold-consistency <file>
                                       Check fold/unfold consistency for all ADTs
  tungsten doctor check type stubs <file>
                                       Detect residual type stubs after elaboration
  tungsten doctor check type constructor-counts <file>
                                       Validate constructor-list integrity for all ADTs
  tungsten doctor check type constructor-stubs <file>
                                       Detect stale constructor stubs (TyVar in encoded types)
  tungsten doctor check type forall-resolution <file>
                                       Detect inner foralls in structural positions (ADR 21.5.26b)  [cost 3]
  tungsten doctor check ir layout <file.ll>
                                       Check store/load type-width consistency  [cost 1]
  tungsten doctor check ir declares --from-existing-ir <dir>
                                       Validate call/declare hygiene in .ll files  [cost 1-2]
  tungsten doctor check reexport-completeness <file>
                                       Check pub use re-export completeness
  tungsten doctor check mono-coverage <file>
                                       Verify all TyApp sites have mono owners  [requires codegen]
  tungsten doctor check module-overlap
                                       Detect foo.rs + foo/mod.rs coexistence (E0761)  [cost 1]
  tungsten doctor check phase-a5 <file>
                                       Check Phase A.5 global collection health  [cost 3]
  tungsten doctor check link-health <binary>
                                       Verify compiled binary stack size + executability  [cost 1]
  tungsten doctor check self-compile-readiness
                                       Pre-flight checks for self-compile (filesystem, linker, LLVM)  [cost 1]
  tungsten doctor check nested-patterns <file>
                                       Detect nested constructor+tuple match patterns (ADR 20.5.26a)  [cost 2]
  tungsten doctor map-span <file> <offset>
                                       Map byte offset to file:line:col  [cost 1]
  tungsten doctor suggest-tools <desc>
                                       Suggest diagnostic tools for an error  [cost 1]"
    );
}
fn print_gdb_debugging() {
    println!(
        "\
\nGDB debugging [cost 4 — requires compiled binary + devcontainer]:
  gdb ./tungsten1                       Debug self-compiled binary
  break <function>$direct               Break on a .tg function (use $direct suffix)
  info registers x0 x1 x2 x3           Inspect arguments (aarch64)
  info registers rdi rsi rdx rcx        Inspect arguments (x86_64)"
    );
}
fn print_sidecar_commands() {
    println!(
        "\
\nSidecar commands [cost 1 — instant, LMDB-backed]:
  tungsten sidecar record-session --error <desc>
                                       Record a debugging session, return session ID
  tungsten sidecar report-outcome --session <id> <cmd> ok|no
                                       Report whether a diagnostic command helped
  tungsten sidecar stats               Show session count, pattern count, top commands
  tungsten sidecar reset               Clear all stored experience data
  tungsten sidecar export --json       Dump full store contents as JSON
  tungsten sidecar start [--repo-root <path>]
                                       Start background sidecar process (Unix domain socket)
  tungsten sidecar stop                Stop the running sidecar process"
    );
}

fn print_comparison_commands() {
    println!(
        "\
\nComparison tools [cost 1–3]:
  tungsten diff ir <a.ll> <b.ll>       Structural IR diff  [cost 1]
  tungsten diff core <a> <b>           Structural Core IR diff  [cost 3]
  tungsten diff types <a> <b> <file>   Structural tree-diff of two type encodings  [cost 3]
  tungsten diff abi <type> <file>      Compare ABI layout: bootstrap vs .tg emitter  [cost 3]
  tungsten diff l1-l2-check <file>     Compare L1 vs tungsten1 check results  [cost 3+3]"
    );
}

fn print_test_command() {
    println!(
        "\
\nTest runner [cost 3–5]:
  tungsten test <file>                 Discover and run test_* functions
  tungsten test <file> --filter <pat>  Filter tests by name substring
  tungsten test <file> --check-only    Run expect_type only (cost 3, no codegen)"
    );
}

fn print_cross_file_diagnostics() {
    println!(
        "\
\nCross-file diagnostic enrichment (ADR 15.5.26a):
  Enriched error types:
    - Argument type mismatch   → cross-file note: 'parameter type declared in `fn`'
    - Return type mismatch     → cross-file note + trace: 'return type declared in `fn`'
  Enrichment requires:
    - Callee is in a different module from the call site
    - Callee's module file is present in the SourceMap
  Limitations:
    - Only direct calls (Expr::App(Expr::Path(...))) are enriched
    - Higher-order calls (let f = get_fn; f()) do not get cross-file notes
    - Note span covers the whole function definition, not just the return type"
    );
}

fn print_profiling() {
    println!(
        "\
\nStructured profiling [cost 4 — ADR 10.5.26j]:
  Build:  cargo build --release -p tungsten_bootstrap --features codegen,profile
  Run:    TUNGSTEN_TRACE_FILE=<path> ./target/release/tungsten compile <file> -o <out>
  Make:   make devcontainer-profile    (orchestrates build + capture)
  Tool:   tungsten-dev profile [--jobs N] [--output <path>]

  Env vars:
    TUNGSTEN_TRACE_FILE=<path>         Chrome Trace JSON output path (default: target/trace.json)
    TUNGSTEN_CODEGEN_JOBS=<n>          Override parallel codegen job count

  Output: Chrome Trace Format JSON → open in https://ui.perfetto.dev
  Traces land in .devcontainer/logs/profiles/ on the host (bind mount)."
    );
}

// ═══════════════════════════════════════════════════════════════════════
// tungsten info types
// ═══════════════════════════════════════════════════════════════════════
