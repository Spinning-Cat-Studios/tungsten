//! `tungsten info` — read-only namespace for querying types, definitions, and encodings.
//!
//! Sub-namespaces (ADR 12.5.26h):
//! - `info type ...` — type inspection commands
//! - `info codegen ...` — codegen-related commands (requires codegen feature)
//! - `info module ...` — module hierarchy commands
//!
//! Legacy flat paths (e.g., `info adt`) remain as hidden aliases for backward
//! compatibility. See ADR 13.4.26d for original design rationale.

pub(crate) mod builtins;
pub(crate) mod cir_sites;
#[cfg(feature = "codegen")]
mod codegen;
// `pub(crate)` only so `test_runner`'s makefile drift guard can share the
// `info pipeline` reconciler's makefile discovery instead of hand-rolling a
// second copy (ADR 31.7.26c close-out). Binary-crate visibility: no public API.
mod cli;
pub(crate) mod commands;
mod commands_detail;
pub(crate) mod error_sites;
mod eval;
mod helpers;
mod module;
// Its only consumer (`info codegen symbols --by-function`) is codegen-gated,
// but the module itself is pure string logic and deliberately is NOT — that is
// what keeps it testable in the LLVM-free build and reachable by the
// coverage/mutation diff gates (ADR 5.8.26b retrospective).
#[cfg_attr(not(feature = "codegen"), allow(dead_code))]
pub(crate) mod symbol_names;
#[cfg(test)]
mod tests;
mod type_info;
mod type_members;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;
use tungsten_bootstrap::driver;

pub use cli::InfoCommands;
pub use eval::InfoEvalCommands;
pub use module::ModuleInfoCommands;

#[cfg(feature = "codegen")]
pub use codegen::{AbiArgs, InfoCodegenCommands};
pub use type_info::{AdtArgs, ConstructorsArgs, InfoTypeCommands, TypeEncodingArgs};
pub use type_members::InfoTypeMembersCommands;

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
        #[cfg(not(feature = "codegen"))]
        InfoCommands::CodegenUnavailable { args } => {
            ExitCode::from(commands::cmd_info_codegen_unavailable(&args))
        }
        InfoCommands::Module(sub) => module::dispatch_module_info(sub, verbose, max_errors),
        InfoCommands::Cir(sub) => dispatch_cir_info(sub),
        InfoCommands::Eval(sub) => eval::dispatch_eval_info(sub, verbose, max_errors),
        InfoCommands::Pipeline { json } => commands::cmd_info_pipeline(json),
        InfoCommands::TryDesugar { name, file } => {
            commands::cmd_info_try_desugar(&name, &file, verbose, max_errors)
        }
        InfoCommands::ErrorEnrichment { file } => {
            commands::cmd_info_error_enrichment(&file, verbose, max_errors)
        }
        InfoCommands::ErrorSites { code } => error_sites::run(&code),
        InfoCommands::Builtins { name } => builtins::run(name.as_deref()),
        InfoCommands::Def {
            name,
            file,
            no_elaborate,
            why_not_certified,
            callers,
        } => {
            if no_elaborate {
                commands::cmd_info_def_parsed(&name, &file)
            } else {
                let reports = commands::DefReports {
                    why_not_certified,
                    callers,
                };
                commands::cmd_info_def(&name, &file, verbose, max_errors, reports)
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
        InfoCommands::ConstructorsLegacy(args) => {
            commands::cmd_info_constructors(&args.name, &args.file, verbose, max_errors, args.raw)
        }
        InfoCommands::MutualRecursionGroupsLegacy { file } => {
            commands::cmd_info_mutual_recursion_groups(&file, verbose, max_errors)
        }
        InfoCommands::FieldTypeLegacy { field_path, file } => {
            commands::cmd_info_field_type(&field_path, &file, verbose, max_errors)
        }
        #[cfg(feature = "codegen")]
        InfoCommands::SymbolsLegacy { file } => {
            commands::cmd_info_symbols(&file, None, verbose, max_errors)
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
    // `info` is read-only inspection, so it forces `Report` enforcement for its
    // own elaboration (ADR 12.8.26a) — the whole namespace, because none of
    // these tools delivers the gate's verdict, so none has a reason to be
    // blocked by it. Caught on `info def --why-not-certified`, added by that ADR
    // to explain E0062 rejections and unable to run on a file that had one.
    // `doctor tool-reachability` carries a row for it.
    let _reporting = tungsten_bootstrap::elaborate::termination::ReportingOnly::begin();
    match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => Some(output),
        Err(e) => {
            eprintln!("error: {e}");
            None
        }
    }
}
