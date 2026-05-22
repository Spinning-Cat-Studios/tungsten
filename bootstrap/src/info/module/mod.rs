//! `tungsten info module` — module-related info subcommands.
//!
//! Groups handlers for `info module tree`, `info module imports`,
//! `info module reexport-chain`, and `info module alias-table` under a
//! single directory that mirrors the CLI sub-namespace.

pub mod alias_table;
pub mod imports;
pub mod reexport_chain;
pub mod tree;

use std::process::ExitCode;

use super::ModuleInfoCommands;

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
    }
}
