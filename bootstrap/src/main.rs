//! `tungsten` — the CLI **binary** crate.
//!
//! Crate split (a common navigation gotcha): this package builds BOTH a library
//! (`tungsten_bootstrap`) and this binary (`tungsten`). The CLI-only modules
//! live HERE in the bin (`info`, `commands`, `compile`, `diff*`, `explain`,
//! `test_runner`, …) and reference the library as `tungsten_bootstrap::…`. The
//! compiler internals — `driver`, `elaborate`, `doctor`, `parser`, `cache` —
//! live in the **lib** (`bootstrap/src/lib.rs`) and reference each other as
//! `crate::…`. A type needing a lib-private field must live in the lib, not
//! here (see `elaborate::ProjectNormalizer`, ADR 21.7.26j).

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

/// Count allocation volume per thread (ADR 8.7.26a §2.2): feeds the per-unit
/// `alloc=` census figure and `doctor check unit-cost`. Pass-through to the
/// system allocator plus one thread-local add per allocation (measured ≤ 2%
/// on a stage-1 baseline — see the ADR's acceptance record).
#[global_allocator]
static GLOBAL_ALLOCATOR: tungsten_core::diagnostics::alloc_counter::CountingAllocator =
    tungsten_core::diagnostics::alloc_counter::CountingAllocator;
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

use cli::{CacheCommands, Cli, Commands, DiffCommands, ExprCommands};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Configure diagnostic hint mode (last flag wins if both specified)
    let hint_mode = match (cli.hints, cli.no_hints) {
        (_, true) => HintMode::Off, // --no-hints takes precedence (last flag wins)
        (true, false) => HintMode::On,
        (false, false) => HintMode::Auto,
    };
    tungsten_bootstrap::driver::diagnostics::set_hint_mode(hint_mode);

    // Termination enforcement (ADR 29.6.26e), before any elaboration starts.
    if let Some(level) = cli.termination.as_deref() {
        use tungsten_bootstrap::elaborate::termination::{set_enforcement, Enforcement};
        set_enforcement(Enforcement::parse(Some(level)));
    }

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
/// Intercept the codegen-requiring `doctor` commands (ADR 1.7.26b §2.2) whose
/// handlers live binary-side in `compile::tco`. Returns `Some(code)` when it
/// handled the command, `None` to fall through to the normal dispatch.
#[cfg(feature = "codegen")]
fn try_codegen_doctor(cli: &Cli) -> Option<ExitCode> {
    use doctor::{CheckCommands, DoctorCommands};
    match &cli.command {
        Some(Commands::Doctor(DoctorCommands::Check(CheckCommands::MonoCoverage { file }))) => {
            Some(compile::check_mono_coverage::cmd_check_mono_coverage(
                file,
                cli.verbose,
                cli.max_errors,
            ))
        }
        Some(Commands::Doctor(DoctorCommands::Check(CheckCommands::ExternMapAmbiguity {
            file,
            json,
        }))) => Some(
            compile::check_extern_map_ambiguity::cmd_check_extern_map_ambiguity(
                file,
                *json,
                cli.verbose,
                cli.max_errors,
            ),
        ),
        Some(Commands::Doctor(DoctorCommands::AuditRecursion { file, source_only })) => {
            Some(compile::tco::cmd_audit_recursion_bridged(
                file,
                *source_only,
                cli.verbose,
                cli.max_errors,
            ))
        }
        Some(Commands::Doctor(DoctorCommands::Check(CheckCommands::TcoCoverage {
            file,
            json,
            risk,
            by_site,
            emit,
            gate,
        }))) => Some(compile::tco::cmd_check_tco_coverage(
            file,
            compile::tco::TcoCoverageOpts {
                json: *json,
                risk_high: risk.as_deref() == Some("high"),
                by_site: *by_site,
                emit: *emit,
                gate: *gate,
            },
            cli.verbose,
            cli.max_errors,
        )),
        Some(Commands::Doctor(DoctorCommands::Check(CheckCommands::UnitCost {
            file,
            json,
            threshold,
            emit_serial_list,
        }))) => Some(compile::unit_cost::cmd_check_unit_cost(
            file,
            &compile::unit_cost::UnitCostOpts {
                json: *json,
                threshold: threshold.clone(),
                emit_serial_list: *emit_serial_list,
            },
            cli.verbose,
            cli.max_errors,
        )),
        _ => None,
    }
}

fn dispatch_command(cli: Cli) -> ExitCode {
    // Rewrite `doctor check codegen <x>` onto the flat variant below before the
    // dispatcher sees it (ADR 13.8.26c review). Done here rather than by adding
    // arms to `try_codegen_doctor` so that function stays untouched: its
    // `ExitCode` returns are unassertable in-process, so every edit to it adds
    // mutants no test can kill.
    #[cfg(feature = "codegen")]
    let cli = Cli {
        command: match cli.command {
            Some(Commands::Doctor(doctor::DoctorCommands::Check(check))) => Some(Commands::Doctor(
                doctor::DoctorCommands::Check(doctor::flatten_codegen_check(check)),
            )),
            other => other,
        },
        ..cli
    };
    #[cfg(feature = "codegen")]
    if let Some(code) = try_codegen_doctor(&cli) {
        return code; // codegen-doctor commands dispatch binary-side (ADR 1.7.26b)
    }
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
            require_tests,
            watchdog,
            color,
            assertion_census,
        }) => {
            let opts = test_runner::TestOptions {
                file: &file,
                filter: filter.as_deref(),
                module: module.as_deref(),
                check_only,
                require_tests,
                watchdog_secs: watchdog,
                color,
                assertion_census,
                verbose: cli.verbose,
                max_errors: cli.max_errors,
                dump_types: cli.dump_types,
            };
            test_runner::cmd_test(&opts)
        }
        #[cfg(feature = "codegen")]
        Some(compile_cmd @ Commands::Compile { .. }) => {
            dispatch_compile(compile_cmd, cli.verbose, cli.max_errors, cli.dump_types)
        }
        // The grouped path and its hidden flat alias resolve to the same
        // values, so each pair has one handler (ADR 19.8.26a).
        Some(Commands::Expr(ExprCommands::Eval { expr }) | Commands::EvalLegacy { expr }) => {
            commands::cmd_eval(&expr, cli.verbose, cli.max_errors)
        }
        Some(Commands::Expr(ExprCommands::Repl) | Commands::ReplLegacy) => commands::cmd_repl(),
        Some(
            Commands::Cache(CacheCommands::CleanProject { file }) | Commands::CleanLegacy { file },
        ) => commands::cmd_clean(cli.verbose, file.as_deref()),
        Some(Commands::Cache(CacheCommands::Stats { file, json })) => {
            commands::cmd_cache_stats(cli.verbose, json, file.as_deref())
        }
        Some(Commands::Cache(CacheCommands::Status { file, json })) => {
            commands::cmd_cache_stats(cli.verbose, json, file.as_deref())
        }
        Some(Commands::Cache(CacheCommands::Inspect { file, mode, json })) => {
            commands::cmd_cache_inspect(&file, &mode, json, cli.verbose)
        }
        Some(Commands::Cache(CacheCommands::Prune { file, target_mb })) => {
            commands::cmd_cache_prune(cli.verbose, target_mb, file.as_deref())
        }
        Some(Commands::Cache(CacheCommands::Clean { dry_run })) => {
            commands::cmd_cache_clean_all(cli.verbose, dry_run)
        }
        Some(Commands::Info(subcmd)) => info::cmd_info(subcmd, cli.verbose, cli.max_errors),
        Some(Commands::Explain(subcmd)) => explain::cmd_explain(subcmd),
        Some(Commands::Sidecar(subcmd)) => tungsten_bootstrap::sidecar::cmd_sidecar(subcmd),
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
        #[cfg(feature = "codegen")]
        DiffCommands::Exec { file, timeout } => {
            diff::exec::cmd_diff_exec(&file, timeout, &diff::exec::ExecOverrides::from_env())
        }
        DiffCommands::Cache {
            file,
            mode,
            timeout,
            gate,
        } => diff::cache::cmd_diff_cache(
            &file,
            &mode,
            timeout,
            gate,
            &diff::cache::CacheOverrides::from_env(),
        ),
        DiffCommands::BootstrapSelfhostCheck {
            file,
            selfhost_binary,
        } => diff::bootstrap_selfhost::cmd_diff_bootstrap_selfhost_check(
            &file,
            &selfhost_binary,
            verbose,
        ),
        DiffCommands::SelfhostCore {
            definition,
            file,
            selfhost_binary,
        } => diff::selfhost_core::cmd_diff_selfhost_core(
            &definition,
            &file,
            &selfhost_binary,
            verbose,
        ),
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

/// Build `CompileFlags` from the parsed `compile` subcommand and dispatch it.
/// Split out of `dispatch_command` to keep that dispatcher within the
/// function-size limit.
#[cfg(feature = "codegen")]
fn dispatch_compile(
    compile_cmd: Commands,
    verbose: bool,
    max_errors: usize,
    dump_types: bool,
) -> ExitCode {
    let Commands::Compile {
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
        only_unit,
        dump_synthesized,
    } = compile_cmd
    else {
        unreachable!("dispatch_compile called with a non-Compile command");
    };
    // Map the optional-value flag: absent → off, bare → all, `=sym` → filter.
    let dump_synthesized = dump_synthesized.map(|s| if s.is_empty() { None } else { Some(s) });
    let flags = compile::CompileFlags {
        emit_llvm,
        verbose,
        max_errors,
        dump_types,
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
            only_units: only_unit,
            dump_synthesized,
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
        codegen_serial_units: compile::parse_codegen_serial_units(),
    };
    compile::cmd_compile(&file, output.as_deref(), &flags)
}
