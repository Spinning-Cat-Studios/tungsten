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

/// Type-related info subcommands (ADR 12.5.26h).
///
/// Grouped under `tungsten info type <subcommand>` to reduce namespace
/// pressure on the top-level `info` command.
#[derive(Subcommand)]
pub enum InfoTypeCommands {
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
    ///
    /// Examples:
    ///   tungsten info type encoding List examples/list.tg
    Encoding {
        /// ADT name
        name: String,

        /// The source file containing the ADT
        file: PathBuf,
    },

    /// Display the μ-type encoding of a named type (ADR 20.4.26c)
    ///
    /// Shows the raw Type tree encoding, with options to show
    /// cached (post-normalization) form and mutual recursion group info.
    ///
    /// Examples:
    ///   tungsten info type type-encoding List examples/list.tg
    TypeEncoding(TypeEncodingArgs),

    /// Inspect constructor list entries for an ADT (ADR 7.5.26e)
    ///
    /// Shows constructor entries with grouping, duplicate detection,
    /// and invariant validation.
    ///
    /// Examples:
    ///   tungsten info type constructors Option examples/option.tg
    Constructors {
        /// ADT name (e.g., "AB", "Option")
        name: String,

        /// The source file containing the ADT
        file: PathBuf,
    },

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

    /// Show stored and resolved types for a record or ADT field (ADR 20.4.26g)
    ///
    /// Displays how the elaborator sees a field's type, showing both the
    /// stored form (from Phase 1c collection) and the resolved form
    /// (after type encoding and μ-substitution).
    ///
    /// Examples:
    ///   tungsten info type field-type List.Cons.tail examples/list.tg
    FieldType {
        /// Field path: Record.field or `ADT.Constructor.field_index`
        field_path: String,

        /// The source file containing the type
        file: PathBuf,
    },

    /// Show record field layout with types and product positions
    ///
    /// Lists all fields of a record type in canonical order, with their
    /// types and product-encoding positions (fst/snd chains).
    ///
    /// Examples:
    ///   tungsten info type record-fields Point examples/hello.tg
    ///   tungsten info type record-fields Config src/compiler/main.tg
    RecordFields {
        /// Record type name (e.g., "Point", "Config")
        name: String,

        /// The source file containing the record type
        file: PathBuf,
    },

    /// Show effective visibility of constructors or fields (ADR 14.5.26c)
    ///
    /// Displays parent type visibility and per-member effective visibility,
    /// showing whether each member inherits or overrides.
    ///
    /// Examples:
    ///   tungsten info type visibility Token examples/option.tg
    ///   tungsten info type visibility Config examples/hello.tg
    Visibility {
        /// Type name (ADT or record)
        name: String,

        /// The source file containing the type
        file: PathBuf,
    },
}

/// Dispatch a type-related info subcommand.
pub fn dispatch_type_info(cmd: InfoTypeCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
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
        InfoTypeCommands::TypeEncoding(args) => commands::cmd_info_type_encoding(
            &args.name,
            &args.file,
            verbose,
            max_errors,
            args.show_raw,
        ),
        InfoTypeCommands::Constructors { name, file } => {
            commands::cmd_info_constructors(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::MutualRecursionGroups { file } => {
            commands::cmd_info_mutual_recursion_groups(&file, verbose, max_errors)
        }
        InfoTypeCommands::FieldType { field_path, file } => {
            commands::cmd_info_field_type(&field_path, &file, verbose, max_errors)
        }
        InfoTypeCommands::RecordFields { name, file } => {
            commands::cmd_info_record_fields(&name, &file, verbose, max_errors)
        }
        InfoTypeCommands::Visibility { name, file } => {
            commands::cmd_info_type_visibility(&name, &file, verbose, max_errors)
        }
    }
}
