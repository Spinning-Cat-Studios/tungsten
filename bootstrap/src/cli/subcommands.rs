//! Cache and Diff CLI subcommand definitions.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum CacheCommands {
    /// Show cache statistics (AST + elaboration)
    ///
    /// Reports the ROOT it inspected (ADR 5.8.26d D5). With no operand that is
    /// the current directory, which is NOT where a build writes: the writer
    /// resolves the root from the entry file's parent, so a run from the repo
    /// root can report `Elab entries: 0` about a project whose cache is
    /// elsewhere. Pass the entry file to inspect that project's cache.
    Stats {
        /// Entry source file whose project cache to inspect (default: current directory)
        file: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect per-module elaboration-cache tiers + run/test body hazard (ADR 4.7.26d)
    ///
    /// For each module in the project, reports the highest cache tier present
    /// (full-output / signature-only / uncached), its cached def count, and — for
    /// the selected mode — whether that entry would serve `CoreDef` bodies to a
    /// `run`/`test`. A signature-only entry answers "No": the ADR 4.7.26c hazard
    /// that `cache status`'s aggregate counts cannot surface.
    ///
    /// Cost 3 (parse + Signature Collection signature collection; no Body Elaboration / no codegen).
    ///
    /// See also: `tungsten cache status` (aggregate counts),
    /// `tungsten diff cache` (cold-vs-warm parity canary), `tungsten cache clean`.
    ///
    /// Examples:
    ///   tungsten cache inspect src/compiler/main.tg --mode run
    ///   tungsten cache inspect examples/hello.tg --json
    #[command(after_help = "See also: `tungsten diff cache`, `tungsten cache status`.")]
    Inspect {
        /// The entry source file of the project to inspect
        file: PathBuf,

        /// Which mode's body hazard to report (run|test|check)
        #[arg(long, default_value = "run")]
        mode: String,

        /// Output as JSON (one record per module)
        #[arg(long)]
        json: bool,
    },

    /// Show cache status summary (alias for stats, ADR 10.5.26l)
    ///
    /// Reports the root it inspected; pass an entry file to inspect that
    /// project's cache rather than the current directory's (ADR 5.8.26d D5).
    ///
    /// Examples:
    ///   tungsten cache status src/compiler/main.tg
    Status {
        /// Entry source file whose project cache to inspect (default: current directory)
        file: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Prune cache to target size (removes least recently used entries)
    ///
    /// Reports the root it pruned; pass an entry file to prune that project's
    /// cache rather than the current directory's (ADR 5.8.26d D5).
    Prune {
        /// Entry source file whose project cache to prune (default: current directory)
        file: Option<PathBuf>,

        /// Target size in MB (defaults to configured `max_size_mb`)
        #[arg(long)]
        target_mb: Option<u64>,
    },

    /// Recursively find and remove ALL .tungsten cache directories (skips target/)
    ///
    /// Scope: every `.tungsten/` under the current directory. This is the broad
    /// one — the remedy after an AST-variant change or a suspected stale-type
    /// problem, and the one that cannot miss a project because you ran it from
    /// the wrong directory.
    ///
    /// NOT the same as `tungsten cache clean-project`, despite the shared verb:
    /// that one clears a SINGLE project's cache, resolved from the cwd or an
    /// entry-file operand (ADR 5.8.26d D5). Use `--dry-run` first if the blast
    /// radius matters.
    #[command(
        after_help = "See also: `tungsten cache clean-project [<file>]` (clears ONE project's cache), `tungsten cache status <file>`."
    )]
    Clean {
        /// Show what would be removed without actually deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Clear ONE project's build cache (ADR 19.8.26a; was `tungsten clean`)
    ///
    /// Scope: a single `.tungsten/` directory. Reports the root it cleared.
    /// With no operand that root is the current directory, which is NOT where a
    /// build writes — the writer resolves it from the entry file's parent, so a
    /// run from the repo root can clear nothing while the project's cache sits
    /// elsewhere (ADR 5.8.26d D5). Pass the entry file to clear that project's.
    ///
    /// NOT the same as its neighbour `tungsten cache clean`, which walks the
    /// tree and removes EVERY `.tungsten/` directory under the cwd. When in
    /// doubt after a compiler change, you almost certainly want that one — this
    /// one cannot reach a project you ran it from the wrong directory for.
    ///
    /// Spelled `clean-project` rather than `clean` because the two now sit
    /// adjacent: flat, the top-level `clean` took its meaning from being
    /// somewhere else; here the name has to carry the scope itself. The old
    /// `tungsten clean [<file>]` spelling remains as a hidden alias.
    #[command(name = "clean-project")]
    #[command(
        after_help = "See also: `tungsten cache clean` (removes ALL .tungsten/ dirs recursively), `tungsten cache status <file>`."
    )]
    CleanProject {
        /// Entry source file whose project cache to clear (default: current directory)
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum DiffCommands {
    /// Compare two LLVM IR files structurally (type defs + function signatures)
    Ir {
        /// Baseline IR file
        file_a: PathBuf,

        /// Candidate IR file
        file_b: PathBuf,

        /// Only compare type definitions
        #[arg(long)]
        types_only: bool,

        /// Only compare function signatures
        #[arg(long)]
        signatures_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compare two Core IR dump files structurally (from --dump-ir output)
    Core {
        /// Baseline Core IR dump file
        file_a: PathBuf,

        /// Candidate Core IR dump file
        file_b: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Structural tree-diff of two type encodings
    ///
    /// Compares the cached encodings of two named types and shows
    /// an inline diff with +/- markers at divergence points.
    /// Returns exit code 0 if identical, nonzero if different.
    ///
    /// Examples:
    ///   tungsten diff types `TypeA` `TypeB` examples/list.tg
    ///   tungsten diff types Expr `TypeExpr` src/compiler/main.tg
    Types {
        /// First type name
        type_a: String,

        /// Second type name
        type_b: String,

        /// The source file containing both types
        file: PathBuf,
    },

    /// Compare ABI layout between bootstrap codegen and .tg emitter (ADR 13.5.26k)
    ///
    /// Elaborates the file, computes the bootstrap codegen layout for the
    /// named type, and compares it with the self-hosted emitter's ABI manifest.
    ///
    /// Examples:
    ///   tungsten diff abi Nat examples/hello.tg
    ///   tungsten diff abi Option src/compiler/main.tg
    #[cfg(feature = "codegen")]
    #[command(
        after_help = "See also: `tungsten info type adt` for ADT details, `tungsten info codegen abi` for IR-level ABI."
    )]
    Abi {
        /// Type name to compare (e.g., "Nat", "Option", "List")
        type_name: String,

        /// The source file to elaborate
        file: PathBuf,
    },

    /// Run a program on the evaluator AND natively, compare outputs (ADR 3.7.26d)
    ///
    /// Compiles the program natively, runs it, runs the same program under
    /// the bootstrap evaluator, and compares stdout (byte-for-byte after
    /// trailing-newline normalization; stderr is reported but not compared).
    /// This is the general detector for silent value miscompiles: binaries
    /// that compile cleanly, pass the LLVM verifier, and print garbage.
    ///
    /// Eligibility (v1): deterministic programs only — empty stdin, no argv,
    /// environment not passed to the native binary beyond PATH/TMPDIR.
    /// Programs reading time, randomness, filesystem state, or the
    /// environment can diverge without a compiler bug.
    ///
    /// Exit codes: 0 parity; 1 output divergence; 2 native runtime error
    /// with evaluator Ok; 3 compile error (neither side ran); 4 evaluator
    /// error with native Ok; 5 timeout on either side.
    ///
    /// Cost 5 (compile + two executions).
    ///
    /// See also: `tungsten doctor check ir sret-stores` (static sret-shape
    /// lint), `tungsten diff bootstrap-selfhost-check`.
    ///
    /// Examples:
    ///   tungsten diff exec tests/dead_arm_letelse_run.tg
    ///   tungsten diff exec examples/hello.tg --timeout 10
    #[cfg(feature = "codegen")]
    Exec {
        /// The source file to run on both sides
        file: PathBuf,

        /// Per-side timeout in seconds (compile, native run, evaluator run)
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },

    /// Cold-vs-warm cache parity check — the cache-poisoning canary (ADR 4.7.26d)
    ///
    /// Runs the program cold (a fresh, isolated cache dir) then warm (a second
    /// run reusing that dir) and compares exit status + stdout. The 4.7.26c bug
    /// was a silent cold-vs-warm divergence (warm reads a bodyless signature
    /// entry → "no tests found" / spurious E0030); this is the automated
    /// detector for that whole class, and the cold-vs-warm analog of
    /// `diff exec`'s native-vs-evaluator check.
    ///
    /// Runs on the evaluator — no LLVM, CI-friendly.
    ///
    /// Exit codes: 0 parity; 1 divergence (cache poisoning); 3 compile /
    /// elaboration error (neither side ran); 5 timeout.
    ///
    /// Cost 5 (two evaluator runs).
    ///
    /// See also: `tungsten cache inspect` (static per-module tier + hazard),
    /// `tungsten diff exec`, `tungsten cache clean`.
    ///
    /// Examples:
    ///   tungsten diff cache src/compiler/main.tg
    ///   tungsten diff cache tests/foo.tg --mode test --gate
    #[command(after_help = "See also: `tungsten cache inspect`, `tungsten diff exec`.")]
    Cache {
        /// The source file to run cold then warm
        file: PathBuf,

        /// Which evaluator mode to run on both sides (run|test)
        #[arg(long, default_value = "run")]
        mode: String,

        /// Per-side timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// CI gate form: non-zero exit only on a genuine divergence
        #[arg(long)]
        gate: bool,
    },

    /// Compare bootstrap and self-host check results on a file (ADR 20.5.26a)
    ///
    /// Runs the bootstrap (this binary) and the self-hosted compiler
    /// (tungsten1, self-compiled) on the same file in check mode, then
    /// compares error counts and messages. Helps identify codegen
    /// regressions in the self-hosted compiler.
    ///
    /// Cost 3+3 (two elaboration passes).
    ///
    /// Examples:
    ///   tungsten diff bootstrap-selfhost-check src/compiler/main.tg --selfhost-binary ./tungsten1
    ///   tungsten diff bootstrap-selfhost-check examples/hello.tg --selfhost-binary ./tungsten1
    #[command(name = "bootstrap-selfhost-check")]
    BootstrapSelfhostCheck {
        /// The source file to check with both compilers
        file: PathBuf,

        /// Path to the tungsten1 (self-compiled) binary
        #[arg(long, default_value = "./tungsten1")]
        selfhost_binary: PathBuf,
    },

    /// Compare one definition's Core TERM across both compilers (ADR 19.8.26d retrospective)
    ///
    /// The sibling above compares error counts, `exec` compares printed
    /// values, `cache` compares cold against warm. None of them can see the
    /// two compilers agreeing on the verdict while disagreeing about the
    /// *term* — the state ADR 19.8.26d had to establish by reading the
    /// lowering source and building a fixture pair, because no command asked.
    ///
    /// Both sides render through the same `Display for Term`, so a textual
    /// comparison is a structural one. Cost 3+3 (two elaborations). Exit 1 on
    /// a divergence, exit 2 when the self-host could not be asked — its
    /// diagnostics stubbed out (the production default), no definition by that
    /// name, or a term it could not read back.
    ///
    /// Agreement on one definition is agreement on one definition. For the
    /// corpus-wide question, see `tungsten doctor check selfhost closed-terms`.
    ///
    /// Read BOTH rendered terms, not the byte offset alone: the two compilers
    /// spell literals and mu-binders differently (`1` vs `succ zero`, `μList`
    /// vs `μα_List`), so the first textual difference is routinely cosmetic
    /// while the structural one is further in.
    ///
    /// Examples:
    ///   tungsten diff selfhost-core main src/compiler/main.tg
    ///   tungsten diff selfhost-core len examples/list.tg --selfhost-binary ./tungsten1
    #[command(name = "selfhost-core")]
    SelfhostCore {
        /// The definition to compare
        definition: String,

        /// The source file both compilers elaborate
        file: PathBuf,

        /// Path to the tungsten1 (self-compiled) binary
        #[arg(long, default_value = "./tungsten1")]
        selfhost_binary: PathBuf,
    },
}
