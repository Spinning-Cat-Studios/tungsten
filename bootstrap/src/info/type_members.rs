//! `tungsten info type members` — what a type declares, and how each declared
//! member is laid out (ADR 19.8.26a).
//!
//! Grouped because `info type` was at 13 of `cli-surface`'s 15 and, like the
//! bootstrap's top-level namespace, had never been counted by any of the three
//! ADRs that spent a month paying down its neighbours under `doctor check`.
//! 15.8.26a met it the expensive way: its preferred option was to re-home two
//! reporters *into* `info type`, which would have put this namespace at exactly
//! 15 — the condition that ADR existed to remove, one namespace over.
//!
//! These four are the subset that reports on a type's **members** rather than
//! on the type itself. Every other `info type` command answers a question about
//! a whole type — its encoding, its μ-chain, its tree size, its position in the
//! resolution order — and takes a bare type name. These take a *member*: a
//! constructor list, a field path, a record's field layout, a per-member
//! visibility census. The seam is visible in the arguments, not only in the
//! prose.
//!
//! | subcommand | the member view it prints |
//! |---|---|
//! | `constructors` | an ADT's constructor entries, with duplicate + invariant detection |
//! | `field-type` | one named field's stored and resolved type |
//! | `record-fields` | a record's fields in canonical order, with product positions |
//! | `visibility` | each constructor's or field's effective visibility |
//!
//! **The encoding cluster was the other candidate and was refused on cost.**
//! `encoding`, `type-encoding`, `size`, `mu-members`, `mutual-recursion-groups`
//! and `encode-order` are six members to these four, so grouping them buys five
//! slots against three — but a grep of the live surfaces (everything outside
//! `notes/`, which is a historical record and is not rewritten) counts **93**
//! mentions of those six spellings against **42** for these four. 15.8.26a's
//! criterion is prose churn, and by it the smaller group wins on the ratio even
//! though it wins fewer slots. The encoding cluster stays available as the next
//! grouping if a future addition needs one.
//!
//! The flat `info type {constructors, field-type, record-fields, visibility}`
//! spellings all remain as hidden aliases on [`super::InfoTypeCommands`].

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use super::{commands, ConstructorsArgs};

/// Member-layout info subcommands (ADR 19.8.26a).
///
/// Grouped under `tungsten info type members <subcommand>`.
#[derive(Subcommand)]
pub enum InfoTypeMembersCommands {
    /// Inspect constructor list entries for an ADT (ADR 7.5.26e)
    ///
    /// Shows constructor entries with grouping, duplicate detection,
    /// and invariant validation.
    ///
    /// `--raw` additionally prints each field's stored `Type` verbatim
    /// (ADR 1.8.26c retrospective). The default rendering goes through
    /// `Display`, which STRIPS the Type-Body Collection `@`-prefix and shows a
    /// μ-binder occurrence as an ordinary name — so `TyVar("List")`,
    /// `TyVar("@List")` and `TyVar("α_List")` are three different things
    /// printed alike, and which one a field holds decides whether
    /// self-reference replacement applies, whether `@`-stripping is needed, or
    /// whether it is bound by an enclosing `Mu`. Each field is labelled
    /// `tyvar` / `at-tyvar` / `mu-binder` / `app/0` / `app/n` / `adt` / `mu` /
    /// `structural`.
    ///
    /// See also: `tungsten doctor check type integrity constructor-counts
    /// <file>` (the same list, checked rather than printed).
    ///
    /// Examples:
    ///   tungsten info type members constructors Option examples/option.tg
    ///   tungsten info type members constructors List examples/list.tg --raw
    Constructors(ConstructorsArgs),

    /// Show stored and resolved types for a record or ADT field (ADR 20.4.26g)
    ///
    /// Displays how the elaborator sees a field's type, showing both the
    /// stored form (from Type-Body Collection collection) and the resolved form
    /// (after type encoding and μ-substitution).
    ///
    /// Needs the file to elaborate successfully, so it cannot inspect the file
    /// that raised the mismatch you are chasing — see
    /// `tungsten compile --trace-normalization=<def>` for that.
    ///
    /// Examples:
    ///   tungsten info type members field-type List.Cons.tail examples/list.tg
    #[command(name = "field-type")]
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
    ///   tungsten info type members record-fields Point examples/hello.tg
    ///   tungsten info type members record-fields Config src/compiler/main.tg
    #[command(name = "record-fields")]
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
    ///   tungsten info type members visibility Token examples/option.tg
    ///   tungsten info type members visibility Config examples/hello.tg
    Visibility {
        /// Type name (ADT or record)
        name: String,

        /// The source file containing the type
        file: PathBuf,
    },
}

/// Dispatch a member-layout info subcommand.
pub fn dispatch_type_members(
    cmd: InfoTypeMembersCommands,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    match cmd {
        InfoTypeMembersCommands::Constructors(args) => {
            commands::cmd_info_constructors(&args.name, &args.file, verbose, max_errors, args.raw)
        }
        InfoTypeMembersCommands::FieldType { field_path, file } => {
            commands::cmd_info_field_type(&field_path, &file, verbose, max_errors)
        }
        InfoTypeMembersCommands::RecordFields { name, file } => {
            commands::cmd_info_record_fields(&name, &file, verbose, max_errors)
        }
        InfoTypeMembersCommands::Visibility { name, file } => {
            commands::cmd_info_type_visibility(&name, &file, verbose, max_errors)
        }
    }
}
