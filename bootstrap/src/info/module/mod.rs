//! `tungsten info module` — module-related info subcommands.
//!
//! Groups handlers for `info module tree`, `info module imports`,
//! `info module reexport-chain`, `info module alias-table`,
//! `info module import-targets` and `info module dependents` under a single
//! directory that mirrors the CLI sub-namespace.

pub mod alias_table;
pub mod commands;
pub mod dependents;
pub mod import_targets;
pub mod imports;
pub mod reexport_chain;
pub mod tree;

pub use commands::ModuleInfoCommands;

use std::process::ExitCode;

/// Dispatch module-related info subcommands.
pub fn dispatch_module_info(cmd: ModuleInfoCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match cmd {
        ModuleInfoCommands::Tree { file } => tree::cmd_info_module_tree(&file, verbose),
        ModuleInfoCommands::Imports { module, file } => {
            imports::cmd_info_imports(&module, &file, verbose, max_errors)
        }
        ModuleInfoCommands::ReexportChain { module, file } => {
            reexport_chain::cmd_info_reexport_chain(&module, &file, verbose)
        }
        ModuleInfoCommands::AliasTable { module, file } => {
            alias_table::cmd_info_alias_table(&module, &file, verbose)
        }
        ModuleInfoCommands::ImportTargets { module, file } => {
            import_targets::cmd_info_import_targets(&module, &file, verbose, max_errors)
        }
        ModuleInfoCommands::Dependents { module, file } => {
            dependents::cmd_info_module_dependents(&module, &file, verbose, max_errors)
        }
    }
}
