//! `tungsten info` — read-only namespace for querying types, definitions, and encodings.
//!
//! Sub-namespaces (ADR 12.5.26h):
//! - `info type ...` — type inspection commands
//! - `info codegen ...` — codegen-related commands (requires codegen feature)
//! - `info module ...` — module hierarchy commands
//!
//! Legacy flat paths (e.g., `info adt`) remain as hidden aliases for backward
//! compatibility. See ADR 13.4.26d for original design rationale.

pub(crate) mod cir_sites;
#[cfg(feature = "codegen")]
mod codegen;
mod commands;
mod commands_detail;
mod helpers;
mod module;
#[cfg(test)]
mod tests;
mod type_info;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;
use tungsten_bootstrap::driver;

#[cfg(feature = "codegen")]
pub use codegen::{AbiArgs, InfoCodegenCommands};
pub use type_info::{AdtArgs, InfoTypeCommands, TypeEncodingArgs};

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

    /// Show definition type signature and Core IR
    ///
    /// Shows both semantic and structural types, plus the Core term.
    ///
    /// Examples:
    ///   tungsten info def main examples/hello.tg
    ///   tungsten info def `list_append` src/compiler/main.tg
    Def {
        /// Definition name (e.g., "main", "`list_append`")
        name: String,

        /// The source file containing the definition
        file: PathBuf,

        /// Show only parsed (surface) signature without elaboration (cost 2 instead of 3)
        #[arg(long)]
        no_elaborate: bool,
    },

    /// Explain the compiler pipeline phases
    ///
    /// Shows compiler stages, key types at each boundary,
    /// and available diagnostic flags per stage.
    Pipeline,

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

    // ── Hidden legacy aliases (ADR 12.5.26h §2.3) ──
    // These preserve backward compatibility with the old flat paths.
    // They share argument structs and dispatch to the same handlers
    // as their grouped counterparts.
    #[command(name = "types", hide = true)]
    TypesLegacy { file: PathBuf },

    #[command(name = "adt", hide = true)]
    AdtLegacy(AdtArgs),

    #[command(name = "encoding", hide = true)]
    EncodingLegacy { name: String, file: PathBuf },

    #[command(name = "type-encoding", hide = true)]
    TypeEncodingLegacy(TypeEncodingArgs),

    #[command(name = "constructors", hide = true)]
    ConstructorsLegacy { name: String, file: PathBuf },

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

/// Module-related info subcommands (ADR 6.5.26a, 8.5.26f).
///
/// Grouped to keep the `info` namespace manageable. Accessed via
/// `tungsten info module <subcommand>`.
#[derive(Subcommand)]
pub enum ModuleInfoCommands {
    /// Visualize the module hierarchy, elaboration order, and cross-branch deps (ADR 6.5.26a)
    ///
    /// Shows the containment tree, dependency-sorted elaboration sequence,
    /// and cross-branch import edges. Cost ≤ 2 (parse only).
    ///
    /// Examples:
    ///   tungsten info module tree `examples/module_example/main.tg`
    ///   tungsten info module tree src/compiler/main.tg
    Tree {
        /// The root source file of the project
        file: PathBuf,
    },

    /// Show import resolution status for a module (ADR 6.5.26a)
    ///
    /// Lists each `use` declaration and whether imported names resolved
    /// to full definitions or stubs after elaboration.
    ///
    /// Examples:
    ///   tungsten info module imports `driver::ffi` src/compiler/main.tg
    ///   tungsten info module imports parser `examples/module_example/main.tg`
    Imports {
        /// Fully qualified module path (e.g., "`driver::ffi`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Trace re-export chain for a module's items (ADR 8.5.26f)
    ///
    /// Shows how items from a module propagate through `pub use`
    /// declarations in the module tree.
    ///
    /// Examples:
    ///   tungsten info module reexport-chain child `examples/module_example/main.tg`
    ///   tungsten info module reexport-chain `elab::env` src/compiler/main.tg
    ReexportChain {
        /// Fully qualified module path (e.g., "child", "`elab::env`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// Show import alias mappings for a module (ADR 16.5.26b)
    ///
    /// Lists each aliased import (`use X as Y`) showing the local alias name,
    /// the original name, and the source path. Aliased names suppress the
    /// original in that module's scope. Cost ≤ 2 (parse only).
    ///
    /// Examples:
    ///   tungsten info module alias-table math `tests/import_alias/main.tg`
    ///   tungsten info module alias-table `driver::ffi` src/compiler/main.tg
    #[command(name = "alias-table")]
    AliasTable {
        /// Fully qualified module path (e.g., "math", "`driver::ffi`")
        module: String,

        /// The root source file of the project
        file: PathBuf,
    },
}

/// CIR inspection subcommands (ADR 13.5.26k).
///
/// Grouped under `tungsten info cir <subcommand>`.
#[derive(Subcommand)]
pub enum CirInfoCommands {
    /// List construction sites for a CIR variant (cost 2, parse only)
    ///
    /// Traverses the parsed AST to find where a specific `CodegenIR`
    /// constructor (e.g., `CIRInl`, `CIRCase`) is applied.
    ///
    /// Examples:
    ///   tungsten info cir sites `CIRInl` src/compiler/main.tg
    ///   tungsten info cir sites `CIRCase` src/compiler/main.tg
    #[command(
        after_help = "See also: `tungsten info type adt CodegenIR <file>` for the full variant list."
    )]
    Sites {
        /// CIR variant name (e.g., "`CIRInl`", "`CIRCase`", "`CIRLambda`")
        variant: String,

        /// The root source file of the project
        file: PathBuf,
    },

    /// List all `CodegenIR` constructors with field counts (cost 2, parse only)
    ///
    /// Parses the CIR types module and enumerates every constructor in the
    /// `CodegenIR` ADT, showing each variant's name and arity.
    ///
    /// Examples:
    ///   tungsten info cir constructors src/compiler/main.tg
    #[command(
        after_help = "See also: `tungsten info cir sites <variant> <file>` to find usage sites."
    )]
    Constructors {
        /// The root source file of the project
        file: PathBuf,
    },
}

/// Dispatch an info subcommand.
pub fn cmd_info(cmd: InfoCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
        InfoCommands::Type(sub) => type_info::dispatch_type_info(sub, verbose, max_errors),
        #[cfg(feature = "codegen")]
        InfoCommands::Codegen(sub) => codegen::dispatch_codegen_info(sub, verbose, max_errors),
        InfoCommands::Module(sub) => module::dispatch_module_info(sub, verbose, max_errors),
        InfoCommands::Cir(sub) => dispatch_cir_info(sub),
        InfoCommands::Pipeline => commands::cmd_info_pipeline(),
        InfoCommands::TryDesugar { name, file } => {
            commands::cmd_info_try_desugar(&name, &file, verbose, max_errors)
        }
        InfoCommands::ErrorEnrichment { file } => {
            commands::cmd_info_error_enrichment(&file, verbose, max_errors)
        }
        InfoCommands::Def {
            name,
            file,
            no_elaborate,
        } => {
            if no_elaborate {
                commands::cmd_info_def_parsed(&name, &file)
            } else {
                commands::cmd_info_def(&name, &file, verbose, max_errors)
            }
        }
        // Legacy aliases delegate to same handlers (ADR 12.5.26h §2.3).
        legacy => dispatch_legacy_info(legacy, verbose, max_errors),
    }
}

/// Dispatch CIR info subcommands (ADR 13.5.26k).
fn dispatch_cir_info(cmd: CirInfoCommands) -> ExitCode {
    match cmd {
        CirInfoCommands::Sites { variant, file } => cir_sites::cmd_cir_sites(&variant, &file),
        CirInfoCommands::Constructors { file } => cir_sites::cmd_cir_constructors(&file),
    }
}

/// Dispatch hidden legacy alias commands.
///
/// Each arm calls the same handler as the corresponding grouped variant,
/// ensuring identical behaviour for old and new paths.
fn dispatch_legacy_info(cmd: InfoCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
        InfoCommands::TypesLegacy { file } => commands::cmd_info_types(&file, verbose, max_errors),
        InfoCommands::AdtLegacy(args) => {
            let opts = commands::AdtInfoOptions {
                verbose,
                max_errors,
                show_fields: args.show_fields,
                check_fold: args.check_fold,
            };
            commands::cmd_info_adt(&args.name, &args.file, &opts)
        }
        InfoCommands::EncodingLegacy { name, file } => {
            commands::cmd_info_encoding(&name, &file, verbose, max_errors)
        }
        InfoCommands::TypeEncodingLegacy(args) => commands::cmd_info_type_encoding(
            &args.name,
            &args.file,
            verbose,
            max_errors,
            args.show_raw,
        ),
        InfoCommands::ConstructorsLegacy { name, file } => {
            commands::cmd_info_constructors(&name, &file, verbose, max_errors)
        }
        InfoCommands::MutualRecursionGroupsLegacy { file } => {
            commands::cmd_info_mutual_recursion_groups(&file, verbose, max_errors)
        }
        InfoCommands::FieldTypeLegacy { field_path, file } => {
            commands::cmd_info_field_type(&field_path, &file, verbose, max_errors)
        }
        #[cfg(feature = "codegen")]
        InfoCommands::SymbolsLegacy { file } => {
            commands::cmd_info_symbols(&file, verbose, max_errors)
        }
        #[cfg(feature = "codegen")]
        InfoCommands::AbiLegacy(args) => crate::dump_abi::cmd_dump_abi(
            args.function_name.as_deref(),
            &args.file,
            args.all,
            args.deep,
        ),
        #[cfg(feature = "codegen")]
        InfoCommands::CodegenUnitsLegacy { file } => {
            codegen::units::cmd_info_codegen_units(&file, verbose, max_errors)
        }
        #[cfg(feature = "codegen")]
        InfoCommands::MonoLegacy { file } => {
            codegen::mono::cmd_info_mono(&file, verbose, max_errors)
        }
        _ => unreachable!("all non-legacy commands matched in cmd_info"),
    }
}

/// Elaborate a project and return the type information needed by info commands.
///
/// Returns a `ProjectOutput` (minus the source map, which info commands don't need)
/// or `None` on error.
pub(crate) fn elaborate_for_info(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> Option<driver::ProjectOutput> {
    match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => Some(output),
        Err(e) => {
            eprintln!("error: {e}");
            None
        }
    }
}
