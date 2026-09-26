//! `tungsten info type` — type inspection sub-namespace (ADR 12.5.26h).
//!
//! Groups 7 type-related info commands under `info type ...`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};

use super::commands::{self, AdtInfoOptions};

/// Shared arguments for `info type adt` / `info adt` (hidden legacy alias).
#[derive(Args)]
pub struct AdtArgs {
    /// ADT name (e.g., "List", "Option")
    pub name: String,

    /// The source file containing the ADT
    pub file: PathBuf,

    /// Show stored vs resolved field types for each constructor
    #[arg(long)]
    pub show_fields: bool,

    /// Check fold/unfold consistency (ADR 21.4.26b)
    #[arg(long)]
    pub check_fold: bool,
}

/// Shared arguments for `info type type-encoding` / `info type-encoding` (hidden legacy alias).
#[derive(Args)]
pub struct TypeEncodingArgs {
    /// Type name (e.g., "List", "Option")
    pub name: String,

    /// The source file containing the type
    pub file: PathBuf,

    /// Show raw (pre-normalization) encoding
    #[arg(long)]
    pub show_raw: bool,
}

/// Arguments for `info type members constructors`, shared with its hidden alias
/// so a flag added to one cannot go missing from the other.
#[derive(Args)]
pub struct ConstructorsArgs {
    /// ADT name (e.g., "AB", "Option")
    pub name: String,

    /// The source file containing the ADT
    pub file: PathBuf,

    /// Print each field's stored `Type` verbatim alongside the rendered form.
    ///
    /// `Display` strips the Type-Body Collection `@`-prefix, so `TyVar("List")`
    /// and `TyVar("@List")` are indistinguishable without this — and the
    /// prefix is exactly what `records()` and `encoded_types` are NOT keyed by
    /// (ADR 1.8.26c).
    #[arg(long)]
    pub raw: bool,
}

/// Type-related info subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten info type <subcommand>` to reduce namespace
/// pressure on the top-level `info` command.
#[derive(Subcommand)]
pub enum InfoTypeCommands {
    /// Inspect a type's members: constructors, fields, and their visibility
    ///
    /// Sub-namespace for the `info type` commands that take a *member* rather
    /// than a whole type (ADR 19.8.26a). See `tungsten info type members
    /// --help`.
    ///
    /// NOT the neighbouring `info type mu-members`, which uses "member" in an
    /// unrelated sense: these are the members a type *declares* — its
    /// constructors and fields, as written in source — while that one reports
    /// the members of a mutual-recursion cluster, the types an encoding's
    /// μ-binders stand for. Source-level structure here, encoding provenance
    /// there.
    #[command(subcommand)]
    #[command(
        after_help = "See also: `tungsten info type mu-members <T> <file>` — a different sense of \
                      \"member\": the SCC members a μ-binder chain denotes, not a type's own \
                      constructors and fields."
    )]
    Members(super::InfoTypeMembersCommands),

    /// List all types defined in a project
    ///
    /// Shows records, ADTs, and type aliases with summaries.
    ///
    /// Examples:
    ///   tungsten info type types examples/hello.tg
    Types {
        /// The source file to inspect
        file: PathBuf,
    },

    /// Show ADT details including constructors and encoding
    ///
    /// Shows constructor fields, encoding strategy, and properties.
    /// Use --show-fields to see stored vs resolved field type representations.
    ///
    /// Examples:
    ///   tungsten info type adt List examples/list.tg
    ///   tungsten info type adt List examples/list.tg --show-fields
    ///   tungsten info type adt List examples/list.tg --check-fold
    Adt(AdtArgs),

    /// Explain encoding strategy for an ADT
    ///
    /// Shows how an ADT is encoded into the Core IR type system,
    /// including constructor layouts and sum/product breakdown.
    /// Shape only — for tree size/occurrence metrics use `info type size`.
    ///
    /// NOT the same thing as `info type type-encoding`, and the two can
    /// legitimately DISAGREE. This prints the encoding derived from the
    /// constructor layout, with type references left as written
    /// (`μα_Rose. Wrap<Rose>`); `type-encoding` prints the CACHED
    /// `encoded_type`, which is what the elaborator actually consumes and may
    /// have collapsed (`μα_Rose. α_Rose`). When they differ, the cached one is
    /// the one that explains a behaviour. Reading only this command is how the
    /// vacuous μ behind E0064 stays invisible (ADR 11.8.26c retrospective).
    ///
    /// Examples:
    ///   tungsten info type encoding List examples/list.tg
    #[command(
        after_help = "See also: `tungsten info type type-encoding <T> <file>` for the CACHED \
                      encoding the elaborator reads — it can differ from this one, and when it \
                      does it is the one that matters."
    )]
    Encoding {
        /// ADT name
        name: String,

        /// The source file containing the ADT
        file: PathBuf,
    },

    /// Show stored-Type-tree size metrics for a named type (ADR 8.7.26a)
    ///
    /// Reports the raw stored tree — total node count, max depth, μ-binder
    /// nesting chain, and α-occurrence count per binder (the kᵢ factors of
    /// the unfold estimate ∏ kᵢ) — plus per-variant stored field-tree node
    /// counts for ADTs. Complements `info type encoding` (shape only).
    ///
    /// Examples:
    ///   tungsten info type size Expr src/compiler/main.tg
    ///   tungsten info type size List examples/list.tg
    Size {
        /// Type name (ADT, record, or alias)
        name: String,

        /// The source file containing the type
        file: PathBuf,
    },

    /// Show a record's declared field count beside its encoded spine (ADR 7.9.26c)
    ///
    /// A record of `n` fields encodes as a right-nested product, so field `i`
    /// is `snd^i` then `fst`. What that convention does not say is what the
    /// spine is made OF: a field whose encoding is structurally a product (an
    /// anonymous tuple, an alias to one, a single-constructor ADT) is SPLICED
    /// into the spine, while a named record or a generic instantiation stays a
    /// reference. So a 3-field record can have a 4-long spine — and a fixture
    /// written to exercise a projection defect reaches it only when it does.
    ///
    /// Two counts, never a verdict: what a gap means depends on the defect
    /// being hunted (D2). Cost 3 (elaboration only).
    ///
    /// Examples:
    ///   tungsten info type spine Cursor src/compiler/main.tg
    #[command(
        after_help = "See also: `docs/repo-memory/elaboration-pipeline.md` \u{a7} What a record's \
                      product spine is made of, and `tungsten info type members record-fields \
                      <T> <file>` for the declared side alone."
    )]
    Spine {
        /// Record type name
        name: String,

        /// The source file containing the record
        file: PathBuf,
    },

    /// Show what each μ-binder in a type's chain denotes (ADR 1.8.26b)
    ///
    /// A mutually recursive cluster encodes as one nested μ-binder per SCC
    /// member, all wrapping the ENTRY member's body — so the encoding does not
    /// carry the other members' bodies. Read as a closed type, an inner binder
    /// appears to denote the entry's body, which is not what that member is;
    /// only μ-provenance knows. This resolves each binder the way synthesis
    /// does, so "what does `α_Beta` actually mean here?" takes one command
    /// instead of an instrumented build.
    ///
    /// Cost 3 (elaboration only).
    ///
    /// Examples:
    ///   tungsten info type mu-members Alpha <repro>.tg
    ///   tungsten info type mu-members Expr src/compiler/main.tg
    #[command(name = "mu-members")]
    #[command(
        after_help = "See also: `tungsten doctor check comparable <T> <file>` reports whether a \
                      comparator can be synthesized over the chain (cost 3).\n\
                      NOT `tungsten info type members` despite the shared word: that one reports \
                      the constructors and fields a type DECLARES; the members here are the \
                      cluster members a μ-binder stands for."
    )]
    MuMembers {
        /// Type name (ADT, record, or alias)
        name: String,

        /// The source file containing the type
        file: PathBuf,
    },

    /// Display the μ-type encoding of a named type (ADR 20.4.26c)
    ///
    /// Shows the raw Type tree encoding, with options to show
    /// cached (post-normalization) form and mutual recursion group info.
    ///
    /// This is the CACHED `encoded_type` — what the elaborator actually
    /// consumes — and it can differ from `info type encoding`, which derives a
    /// view from the constructor layout. For a nested inductive family the two
    /// disagree exactly where it matters: `info type encoding` shows
    /// `μα_Rose. Wrap<Rose>` while this shows the vacuous `μα_Rose. α_Rose`
    /// that no unfold can flatten (the E0064 mechanism, ADR 11.8.26c).
    ///
    /// Blocked by its own gate on a file E0064 rejects: this elaborates the
    /// file first, so it cannot report on a program that matches on a nested
    /// family. Delete the `match` to inspect the chain — or use
    /// `doctor check type vacuous-mu`, which finds such types BEFORE anything
    /// matches on one. (`doctor tool-reachability` carries this pairing.)
    ///
    /// Examples:
    ///   tungsten info type type-encoding List examples/list.tg
    #[command(
        after_help = "See also: `tungsten info type encoding <T> <file>` for the \
                      constructor-layout view (can differ — this one is what the elaborator \
                      reads), and `tungsten doctor check type vacuous-mu <file>` to find \
                      types whose cached encoding collapsed to `μX. X`."
    )]
    TypeEncoding(TypeEncodingArgs),

    /// Display mutual recursion groups (SCC analysis) (ADR 20.4.26c)
    ///
    /// Shows strongly connected components of the type dependency graph,
    /// including μ-binder order, dependency edges, and self-recursive types.
    ///
    /// Examples:
    ///   tungsten info type mutual-recursion-groups examples/list.tg
    MutualRecursionGroups {
        /// The source file to inspect
        file: PathBuf,
    },

    /// Show the deterministic Phase-1d/1e resolution order (ADR 22.7.26d)
    ///
    /// Answers "why does type X resolve/encode before type Y?". Prints the
    /// reverse-topological order (referents before referrers) with each type's
    /// position, and flags multi-member SCCs — cycles where topology cannot
    /// order the members, so they resolve in lexicographic order.
    ///
    /// See also: `tungsten info type mutual-recursion-groups`,
    /// `tungsten doctor check type determinism normalization`.
    ///
    /// Examples:
    ///   tungsten info type encode-order src/compiler/main.tg
    ///   tungsten info type encode-order src/compiler/main.tg --focus my-type
    ///   tungsten info type encode-order examples/list.tg --all
    EncodeOrder {
        /// The source file to inspect (use `.` for the entry file)
        file: PathBuf,

        /// Focus on one type: its position, SCC, and referent positions
        #[arg(long)]
        focus: Option<String>,

        /// Print the full linear order (default: summary + multi-member SCCs)
        #[arg(long)]
        all: bool,
    },

    /// Show an ADT's LLVM layout via each lowering route (ADR 12.7.26c)
    ///
    /// Lowers the named ADT through every applicable route (named / app /
    /// structural / flat-adt) and prints each layout, flagging divergence.
    /// Requires the `codegen` feature.
    ///
    /// See also: `tungsten doctor check type lowering-consistency` (project-wide gate).
    ///
    /// Examples:
    ///   tungsten info type lowering CompareResult tests/comparator_codegen_run.tg
    #[cfg(feature = "codegen")]
    Lowering {
        /// ADT name (e.g., "CompareResult", "Option")
        name: String,

        /// The source file containing the ADT
        file: PathBuf,
    },

    // ── Hidden legacy aliases (ADR 19.8.26a) ──
    // The flat spellings the `members` grouping replaced. Exempt from the
    // `cli-surface` count, so keeping every existing caller working is free.
    #[command(name = "constructors", hide = true)]
    ConstructorsLegacy(ConstructorsArgs),

    #[command(name = "field-type", hide = true)]
    FieldTypeLegacy { field_path: String, file: PathBuf },

    #[command(name = "record-fields", hide = true)]
    RecordFieldsLegacy { name: String, file: PathBuf },

    #[command(name = "visibility", hide = true)]
    VisibilityLegacy { name: String, file: PathBuf },
}

/// Dispatch a type-related info subcommand.
pub fn dispatch_type_info(cmd: InfoTypeCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
        InfoTypeCommands::Members(sub) => {
            super::type_members::dispatch_type_members(sub, verbose, max_errors)
        }
        // The flat aliases carry the same values, so each pair reaches the
        // grouped dispatcher rather than a second copy of the handler call —
        // a misroute here would be invisible in `--help` (ADR 19.8.26a).
        InfoTypeCommands::ConstructorsLegacy(args) => super::type_members::dispatch_type_members(
            super::InfoTypeMembersCommands::Constructors(args),
            verbose,
            max_errors,
        ),
        InfoTypeCommands::FieldTypeLegacy { field_path, file } => {
            super::type_members::dispatch_type_members(
                super::InfoTypeMembersCommands::FieldType { field_path, file },
                verbose,
                max_errors,
            )
        }
        InfoTypeCommands::RecordFieldsLegacy { name, file } => {
            super::type_members::dispatch_type_members(
                super::InfoTypeMembersCommands::RecordFields { name, file },
                verbose,
                max_errors,
            )
        }
        InfoTypeCommands::VisibilityLegacy { name, file } => {
            super::type_members::dispatch_type_members(
                super::InfoTypeMembersCommands::Visibility { name, file },
                verbose,
                max_errors,
            )
        }
        InfoTypeCommands::Types { file } => commands::cmd_info_types(&file, verbose, max_errors),
        InfoTypeCommands::Adt(args) => {
            let opts = AdtInfoOptions {
                verbose,
                max_errors,
                show_fields: args.show_fields,
                check_fold: args.check_fold,
            };
            commands::cmd_info_adt(&args.name, &args.file, &opts)
        }
        InfoTypeCommands::Encoding { name, file } => {
            commands::cmd_info_encoding(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::Size { name, file } => {
            commands::cmd_info_type_size(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::Spine { name, file } => {
            commands::cmd_info_type_spine(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::MuMembers { name, file } => {
            commands::cmd_info_mu_members(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::TypeEncoding(args) => commands::cmd_info_type_encoding(
            &args.name,
            &args.file,
            verbose,
            max_errors,
            args.show_raw,
        ),
        InfoTypeCommands::MutualRecursionGroups { file } => {
            commands::cmd_info_mutual_recursion_groups(&file, verbose, max_errors)
        }
        InfoTypeCommands::EncodeOrder { file, focus, all } => {
            commands::cmd_info_encode_order(&file, focus.as_deref(), all, verbose, max_errors)
        }
        #[cfg(feature = "codegen")]
        InfoTypeCommands::Lowering { name, file } => {
            commands::cmd_info_type_lowering(&name, &file, verbose, max_errors)
        }
    }
}
