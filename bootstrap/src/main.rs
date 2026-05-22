// Clippy lint policy: inherit suppression from lib.rs for the binary crate.
// See ADR 18.5.26h for triage decisions.
#![allow(
    clippy::similar_names,
    clippy::ptr_arg,
    clippy::items_after_statements,
    clippy::match_same_arms,
    clippy::manual_let_else,
    clippy::struct_excessive_bools,
    clippy::needless_for_each,
    clippy::enum_variant_names,
    clippy::too_many_lines,
    clippy::only_used_in_recursion
)]

//! Tungsten Bootstrap Compiler — CLI Driver
//!
//! This is the command-line interface for the Tungsten bootstrap compiler.
//! It provides commands for type-checking, running, compiling, and interacting with
//! Tungsten source files.

use clap::Parser;
use std::process::ExitCode;
use tungsten_bootstrap::doctor;
use tungsten_bootstrap::driver::diagnostics::hints::HintMode;
mod cli;
mod commands;
#[cfg(feature = "codegen")]
mod compile;
mod diff;
mod diff_core;
mod diff_ir;

#[cfg(feature = "codegen")]
mod dump_abi;
mod explain;
mod info;
mod list_commands;
mod test_runner;
mod typo_suggest;

use cli::{CacheCommands, Cli, Commands, DiffCommands};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Configure diagnostic hint mode (last flag wins if both specified)
    let hint_mode = match (cli.hints, cli.no_hints) {
        (_, true) => HintMode::Off, // --no-hints takes precedence (last flag wins)
        (true, false) => HintMode::On,
        (false, false) => HintMode::Auto,
    };
    tungsten_bootstrap::driver::diagnostics::set_hint_mode(hint_mode);

    // Handle direct file argument: `tungsten hello.tg` → `tungsten run hello.tg`
    if let Some(file) = cli.file {
        // If the file doesn't exist and looks like a mistyped subcommand, suggest it.
        if !file.exists() {
            let name = file.to_string_lossy();
            if let Some(suggestion) = typo_suggest::suggest_subcommand(&name) {
                eprintln!("error: file '{name}' not found\n");
                eprintln!("  tip: a similar subcommand exists: '{suggestion}'");
                eprintln!("  try: tungsten {suggestion} --help");
                return ExitCode::FAILURE;
            }
        }
        return commands::cmd_run(&file, cli.verbose, false, cli.max_errors, cli.dump_types);
    }

    dispatch_command(cli)
}

/// Dispatch a parsed CLI command to the appropriate handler.
fn dispatch_command(cli: Cli) -> ExitCode {
    match cli.command {
        Some(Commands::Check {
            file,
            no_cache,
            json,
        }) => {
            let opts = commands::CheckOptions {
                verbose: cli.verbose,
                no_cache,
                max_errors: cli.max_errors,
                dump_types: cli.dump_types,
                json,
            };
            commands::cmd_check(&file, &opts)
        }
        Some(Commands::Run { file, no_cache }) => {
            commands::cmd_run(&file, cli.verbose, no_cache, cli.max_errors, cli.dump_types)
        }
        Some(Commands::Test {
            file,
            filter,
            module,
            check_only,
            color,
        }) => {
            let opts = test_runner::TestOptions {
                file: &file,
                filter: filter.as_deref(),
                module: module.as_deref(),
                check_only,
                color,
                verbose: cli.verbose,
                max_errors: cli.max_errors,
                dump_types: cli.dump_types,
            };
            test_runner::cmd_test(&opts)
        }
        #[cfg(feature = "codegen")]
        Some(Commands::Compile {
            file,
            output,
            emit_llvm,
            check_tyvar_escape,
            codegen_backtrace,
            dump_ir,
            trace_types,
            dump_encoding,
            debug_info,
            sanitize,
            trace_adt_ops,
            trace_encoding,
            trace_normalization,
            trace_constructor_registration,
            trace_musttail,
            trace_escape,
            trace_mono,
            named_lambdas,
            no_codegen,
            alloc_profile,
        }) => {
            let flags = compile::CompileFlags {
                emit_llvm,
                verbose: cli.verbose,
                max_errors: cli.max_errors,
                dump_types: cli.dump_types,
                debug_info,
                sanitize,
                named_lambdas,
                no_codegen,
                diagnostics: compile::DiagnosticFlags {
                    dump_ir,
                    trace_types,
                    dump_encoding,
                    codegen_backtrace,
                    check_tyvar_escape,
                    alloc_profile,
                    tracing: compile::TraceFlags {
                        trace_adt_ops,
                        trace_encoding,
                        trace_normalization,
                        trace_constructor_registration,
                        trace_musttail,
                        trace_escape,
                        trace_mono,
                    },
                },
                codegen_jobs: parse_codegen_jobs(),
            };
            compile::cmd_compile(&file, output.as_deref(), &flags)
        }
        Some(Commands::Eval { expr }) => commands::cmd_eval(&expr, cli.verbose, cli.max_errors),
        Some(Commands::Repl) => commands::cmd_repl(),
        Some(Commands::Clean) => commands::cmd_clean(cli.verbose),
        Some(Commands::Cache(CacheCommands::Stats { json })) => {
            commands::cmd_cache_stats(cli.verbose, json)
        }
        Some(Commands::Cache(CacheCommands::Status { json })) => {
            commands::cmd_cache_stats(cli.verbose, json)
        }
        Some(Commands::Cache(CacheCommands::Prune { target_mb })) => {
            commands::cmd_cache_prune(cli.verbose, target_mb)
        }
        Some(Commands::Cache(CacheCommands::Clean { dry_run })) => {
            commands::cmd_cache_clean_all(cli.verbose, dry_run)
        }
        Some(Commands::Info(subcmd)) => info::cmd_info(subcmd, cli.verbose, cli.max_errors),
        Some(Commands::Explain(subcmd)) => explain::cmd_explain(subcmd),
        Some(Commands::Sidecar(subcmd)) => tungsten_bootstrap::sidecar::cmd_sidecar(subcmd),
        #[cfg(feature = "codegen")]
        Some(Commands::Doctor(doctor::DoctorCommands::Check(
            doctor::CheckCommands::MonoCoverage { file },
        ))) => compile::check_mono_coverage::cmd_check_mono_coverage(
            &file,
            cli.verbose,
            cli.max_errors,
        ),
        Some(Commands::Doctor(subcmd)) => doctor::cmd_doctor(subcmd, cli.verbose),
        Some(Commands::Diff(subcmd)) => dispatch_diff(subcmd, cli.verbose, cli.max_errors),
        Some(Commands::ListCommands { json, tree }) => dispatch_list_commands(json, tree),
        // Hidden backward-compat aliases
        Some(Commands::DiffIr {
            file_a,
            file_b,
            types_only,
            signatures_only,
            json,
        }) => diff_ir::cmd_diff_ir(&file_a, &file_b, types_only, signatures_only, json),
        Some(Commands::DiffCore {
            file_a,
            file_b,
            json,
        }) => diff_core::cmd_diff_core(&file_a, &file_b, json),
        #[cfg(feature = "codegen")]
        Some(Commands::DumpAbi {
            function_name,
            file,
            all,
            deep,
        }) => dump_abi::cmd_dump_abi(function_name.as_deref(), &file, all, deep),
        None => {
            use clap::CommandFactory;
            Cli::command().print_help().unwrap();
            println!();
            ExitCode::SUCCESS
        }
    }
}

/// Dispatch `tungsten diff` subcommands.
fn dispatch_diff(subcmd: DiffCommands, verbose: bool, max_errors: usize) -> ExitCode {
    match subcmd {
        DiffCommands::Ir {
            file_a,
            file_b,
            types_only,
            signatures_only,
            json,
        } => diff_ir::cmd_diff_ir(&file_a, &file_b, types_only, signatures_only, json),
        DiffCommands::Core {
            file_a,
            file_b,
            json,
        } => diff_core::cmd_diff_core(&file_a, &file_b, json),
        DiffCommands::Types {
            type_a,
            type_b,
            file,
        } => doctor::diff_types::cmd_diff_types(&type_a, &type_b, &file, verbose, max_errors),
        #[cfg(feature = "codegen")]
        DiffCommands::Abi { type_name, file } => {
            diff::abi::cmd_diff_abi(&type_name, &file, verbose, max_errors)
        }
        DiffCommands::L1L2Check { file, l2_binary } => {
            diff::l1_l2::cmd_diff_l1_l2_check(&file, &l2_binary, verbose)
        }
    }
}

/// Dispatch `tungsten commands` with output format selection.
fn dispatch_list_commands(json: bool, tree: bool) -> ExitCode {
    use clap::CommandFactory;
    let cmd = Cli::command();
    if json {
        print!("{}", list_commands::list_commands_json(&cmd));
    } else if tree {
        print!("{}", list_commands::list_commands_tree(&cmd));
    } else {
        print!("{}", list_commands::list_commands_flat(&cmd, ""));
    }
    ExitCode::SUCCESS
}

/// Parse TUNGSTEN_CODEGEN_JOBS env var (default: num_cpus, minimum: 1).
#[cfg(feature = "codegen")]
fn parse_codegen_jobs() -> usize {
    compile::parse_codegen_jobs()
}
