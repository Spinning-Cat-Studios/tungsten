//! The `tungsten info` subcommand tree.
//!
//! Split from `info/mod.rs` by the ADR 14.8.26g retrospective: that file sat
//! at 399 of its 400-line budget, so the next subcommand — any subcommand —
//! tripped the size gate. The enum carries each command's `long_about` and
//! `after_help`, which are the surface a human reads via `--help`, so it grows
//! with every tool and belongs apart from the dispatch.

use std::path::PathBuf;

use clap::Subcommand;

#[cfg(feature = "codegen")]
use super::{AbiArgs, InfoCodegenCommands};
use super::{
    AdtArgs, CirInfoCommands, ConstructorsArgs, InfoEvalCommands, InfoTypeCommands,
    ModuleInfoCommands, TypeEncodingArgs,
};

#[derive(Subcommand)]
pub enum InfoCommands {
    // ── Visible grouped sub-namespaces ──
    /// Inspect types, ADTs, encodings, and constructors
    ///
    /// Sub-namespace for type system inspection commands.
    /// See `tungsten info type --help` for details.
    #[command(subcommand)]
    Type(InfoTypeCommands),

    /// Inspect codegen units, mono requests, ABI, and symbols
    ///
    /// Sub-namespace for codegen-related inspection commands.
    /// See `tungsten info codegen --help` for details.
    #[cfg(feature = "codegen")]
    #[command(subcommand)]
    Codegen(InfoCodegenCommands),

    /// Stand-in for `info codegen` in a build without the `codegen` feature.
    ///
    /// Without this, clap answers `unrecognized subcommand 'codegen'` and
    /// suggests `encoding` — an unrelated command — giving no hint that the
    /// cause is a missing build feature rather than a typo. That failure is
    /// easy to hit because the host `make` targets rebuild
    /// `target/debug/tungsten` *without* codegen, silently replacing a
    /// codegen-featured binary (ADR 5.8.26c retrospective: the documented
    /// `info codegen symbols --by-function` route was abandoned for `nm`
    /// because of it).
    ///
    /// Hidden, so it stays out of `--help` and the `commands` listing — the
    /// subcommand genuinely is unavailable in this build, and a visible entry
    /// would also owe the ADR 28.7.26f inventory a classification.
    #[cfg(not(feature = "codegen"))]
    #[command(name = "codegen", hide = true, disable_help_flag = true)]
    CodegenUnavailable {
        /// Swallowed so `info codegen symbols --by-function f` reaches the
        /// explanation instead of failing on an unknown argument first.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
        args: Vec<String>,
    },

    /// Visualize, inspect, and debug the module system
    ///
    /// Sub-namespace for module hierarchy, import resolution, and re-export
    /// chain inspection. See `tungsten info module --help` for details.
    #[command(subcommand)]
    Module(ModuleInfoCommands),

    /// Inspect CIR (Codegen IR) construction sites
    ///
    /// Sub-namespace for CIR variant inspection commands.
    /// See `tungsten info cir --help` for details.
    #[command(subcommand)]
    Cir(CirInfoCommands),

    /// Trace and inspect the evaluator (ADR 21.7.26j)
    ///
    /// Sub-namespace for evaluator inspection. See `tungsten info eval --help`.
    #[command(subcommand)]
    Eval(InfoEvalCommands),

    /// Show definition type signature and Core IR
    ///
    /// Shows both semantic and structural types, plus the Core term.
    ///
    /// `--why-not-certified` adds the termination view: per parameter, whether
    /// it is a candidate decreasing root, and — when it is not — which class of
    /// type refused it. That last part is the reason the flag exists: `Display`
    /// renders a mutual-cluster marker and a real ADT identically, so the type
    /// alone cannot tell you why a recursion that looks structural was rejected
    /// (ADR 12.8.26a).
    ///
    /// See also: `tungsten doctor check type termination <file>` (the whole
    /// project's census), `tungsten explain error E0062` (the rule).
    ///
    /// Examples:
    ///   tungsten info def main examples/hello.tg
    ///   tungsten info def `list_append` src/compiler/main.tg
    ///   tungsten info def `strip_spans_param` src/compiler/main.tg --why-not-certified
    Def {
        /// Definition name (e.g., "main", "`list_append`")
        name: String,

        /// The source file containing the definition
        file: PathBuf,

        /// Show only parsed (surface) signature without elaboration (cost 2 instead of 3)
        #[arg(long)]
        no_elaborate: bool,

        /// Report each parameter's eligibility as a decreasing root (ADR 12.8.26a)
        #[arg(long)]
        why_not_certified: bool,

        /// Report which definitions call this one — "none" is the answer grep
        /// cannot give, because an export is not a call (ADR 12.8.26b)
        #[arg(long)]
        callers: bool,
    },

    /// Explain the compiler pipeline phases
    ///
    /// Shows compiler stages, key types at each boundary,
    /// and available diagnostic flags per stage.
    ///
    /// The inventory is reconciled against the clap command tree by a test
    /// (ADR 28.7.26f), so an undocumented or renamed subcommand fails the build
    /// rather than silently staling this listing.
    ///
    /// See also: `tungsten commands` for the structural listing.
    Pipeline {
        /// Emit the inventory as JSON instead of the rendered listing
        #[arg(long)]
        json: bool,
    },

    /// Show `?` operator desugaring for a definition (ADR 13.5.26e)
    ///
    /// Finds `?` desugaring patterns in the elaborated Core IR and
    /// displays each one with its scrutinee, error branch, and success path.
    ///
    /// Examples:
    ///   tungsten info try-desugar process examples/result.tg
    ///   tungsten info try-desugar `handle_input` src/compiler/main.tg
    TryDesugar {
        /// Definition name (e.g., \"process\")
        name: String,

        /// The source file containing the definition
        file: PathBuf,
    },

    /// Show cross-file diagnostic enrichment points (ADR 15.5.26a)
    ///
    /// Reports which function calls in a file would receive cross-file
    /// diagnostic notes when type errors occur, and which public functions
    /// defined here would enrich errors in other modules.
    ///
    /// Examples:
    ///   tungsten info error-enrichment src/compiler/elab/exprs/mod.tg
    ///   tungsten info error-enrichment `examples/module_example/main.tg`
    #[command(
        after_help = "See also: `tungsten info pipeline` for enrichment capabilities overview."
    )]
    ErrorEnrichment {
        /// The source file to analyze
        file: PathBuf,
    },

    /// Show where an error code is raised in the compiler (cost 1).
    ///
    /// Resolves a code (or a kind name) to its `ElabErrorKind` variant, the
    /// `ElabError` constructor(s) that build it, and every site in the
    /// compiler's own source that raises it — attributed to the enclosing
    /// function, which is the boundary a cascading diagnostic comes from.
    ///
    /// Reads the compiler's Rust source, not a user file: no parse, no
    /// elaboration. The error-module plumbing (the code table, the
    /// constructors) and test files are excluded — they mention every
    /// variant without raising any.
    ///
    /// Examples:
    ///   tungsten info error-sites E0013
    ///   tungsten info error-sites `ExpectedFunction`
    #[command(
        after_help = "See also: `tungsten explain error <code>` for what the code MEANS, \n\
                      `tungsten doctor suggest-tools` when you have a symptom rather than a code."
    )]
    ErrorSites {
        /// Error code (`E0013`, case-insensitive) or kind name (`ExpectedFunction`)
        code: String,
    },

    /// Show which bare names each compiler intercepts before name resolution (cost 1).
    ///
    /// Both compilers match a call's bare name against a fixed table *before*
    /// consulting the value environment, and the two tables are not the same.
    /// A name in only one of them still resolves normally in the other — so any
    /// `.tg` definition carrying it silently becomes that compiler's meaning of
    /// the name, with matching arity and matching types and no diagnostic.
    ///
    /// This is also why `info def <name> --callers` can truthfully answer `none`
    /// about a function the self-hosted compiler resolves to everywhere: the
    /// bootstrap intercepts every call site, so nothing ever resolves to it here.
    ///
    /// With no argument, prints both tables and marks each asymmetry with the
    /// declared reason that makes it safe, or `UNDECLARED`.
    ///
    /// Examples:
    ///   tungsten info builtins
    ///   tungsten info builtins substring
    #[command(
        after_help = "See also: `tungsten info def <name> <file> --callers` for why a builtin's \n\
                      `.tg` namesake reports no callers, \n\
                      `selfhost-conformance --interception-tables` for the gate that fails on a \n\
                      new asymmetry (ADR 20.8.26c)."
    )]
    Builtins {
        /// Report one name instead of the whole listing
        name: Option<String>,
    },

    // ── Hidden legacy aliases (ADR 12.5.26h §2.3) ──
    // These preserve backward compatibility with the old flat paths.
    // They share argument structs and dispatch to the same handlers
    // as their grouped counterparts.
    #[command(name = "types", hide = true)]
    TypesLegacy { file: PathBuf },

    #[command(name = "adt", hide = true)]
    AdtLegacy(AdtArgs),

    /// Deprecated alias — use `tungsten info type encoding` (ADR 8.7.26a §2.5)
    #[command(name = "encoding", hide = true)]
    EncodingLegacy { name: String, file: PathBuf },

    /// Deprecated alias — use `tungsten info type type-encoding` (ADR 8.7.26a §2.5)
    #[command(name = "type-encoding", hide = true)]
    TypeEncodingLegacy(TypeEncodingArgs),

    #[command(name = "constructors", hide = true)]
    ConstructorsLegacy(ConstructorsArgs),

    #[command(name = "mutual-recursion-groups", hide = true)]
    MutualRecursionGroupsLegacy { file: PathBuf },

    #[command(name = "field-type", hide = true)]
    FieldTypeLegacy { field_path: String, file: PathBuf },

    #[cfg(feature = "codegen")]
    #[command(name = "symbols", hide = true)]
    SymbolsLegacy { file: PathBuf },

    #[cfg(feature = "codegen")]
    #[command(name = "abi", hide = true)]
    AbiLegacy(AbiArgs),

    #[cfg(feature = "codegen")]
    #[command(name = "codegen-units", hide = true)]
    CodegenUnitsLegacy { file: PathBuf },

    #[cfg(feature = "codegen")]
    #[command(name = "mono", hide = true)]
    MonoLegacy { file: PathBuf },
}
