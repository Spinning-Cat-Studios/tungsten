//! CLI command handlers for run, eval, repl, clean, and cache operations.

mod cache;
pub use cache::{cmd_cache_clean_all, cmd_cache_prune, cmd_cache_stats, cmd_clean};

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::diagnostics::hints::HintMode;
use tungsten_bootstrap::driver::{self, Mode, PipelineOpts, PipelineResult};

/// Options for the check command.
pub struct CheckOptions {
    pub verbose: bool,
    pub no_cache: bool,
    pub max_errors: usize,
    pub dump_types: bool,
    pub json: bool,
}

/// Check command: type-check a file.
pub fn cmd_check(file: &PathBuf, opts: &CheckOptions) -> ExitCode {
    let CheckOptions {
        verbose,
        no_cache,
        max_errors,
        dump_types,
        json,
    } = *opts;
    // In JSON mode, force hints on (they go in the structured output)
    if json {
        tungsten_bootstrap::driver::diagnostics::set_hint_mode(HintMode::Off);
    }

    let pipeline_opts = PipelineOpts {
        mode: Mode::Check,
        verbose,
        dump_types,
    };
    match driver::run_file_with_options(file, &pipeline_opts, no_cache, max_errors) {
        Ok(PipelineResult::Checked {
            num_defs,
            has_sorry,
        }) => {
            if json {
                // Empty errors in JSON mode = success
                let report = tungsten_bootstrap::driver::diagnostics::hints::JsonDiagnosticReport {
                    errors: vec![],
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else if has_sorry {
                println!(
                    "⚠ {}: {} definition(s), contains sorry",
                    file.display(),
                    num_defs
                );
            } else {
                println!("✓ {}: {} definition(s), all OK", file.display(), num_defs);
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => {
            if json {
                let report = tungsten_bootstrap::driver::diagnostics::hints::JsonDiagnosticReport {
                    errors: vec![tungsten_bootstrap::driver::diagnostics::hints::JsonError {
                        code: "E9999".to_string(),
                        message: "elaboration failed (use non-JSON mode for detailed errors)"
                            .to_string(),
                        file: Some(file.display().to_string()),
                        line: None,
                        hints: vec![tungsten_bootstrap::driver::diagnostics::hints::JsonHint {
                            command: "tungsten doctor suggest-tools \"elaboration error\""
                                .to_string(),
                            reason: "Get diagnostic suggestions for elaboration failures"
                                .to_string(),
                        }],
                    }],
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            }
            ExitCode::FAILURE
        }
        Ok(PipelineResult::Evaluated { .. }) => {
            // Shouldn't happen in check mode
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Tested { .. }) => {
            // Shouldn't happen in check mode
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json {
                let report = tungsten_bootstrap::driver::diagnostics::hints::JsonDiagnosticReport {
                    errors: vec![tungsten_bootstrap::driver::diagnostics::hints::JsonError {
                        code: "E9999".to_string(),
                        message: format!("{e}"),
                        file: Some(file.display().to_string()),
                        line: None,
                        hints: vec![],
                    }],
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::from(3) // IO error
        }
    }
}

/// Run command: type-check and evaluate `main()`.
pub fn cmd_run(
    file: &PathBuf,
    verbose: bool,
    no_cache: bool,
    max_errors: usize,
    dump_types: bool,
) -> ExitCode {
    let pipeline_opts = PipelineOpts {
        mode: Mode::Run,
        verbose,
        dump_types,
    };
    match driver::run_file_with_options(file, &pipeline_opts, no_cache, max_errors) {
        Ok(PipelineResult::Evaluated { value, ty }) => {
            let value_str = driver::format_value(&value);
            if verbose {
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            } else {
                println!("{value_str}");
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Checked { .. }) => {
            // Shouldn't happen in run mode
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Tested { .. }) => {
            // Shouldn't happen in run mode
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(3)
        }
    }
}

/// Eval command: evaluate a single expression.
pub fn cmd_eval(expr: &str, verbose: bool, max_errors: usize) -> ExitCode {
    match driver::eval_expr(expr, verbose, max_errors) {
        Ok(PipelineResult::Evaluated { value, ty }) => {
            let value_str = driver::format_value(&value);
            if verbose {
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            } else {
                println!("{value_str}");
            }
            ExitCode::SUCCESS
        }
        Ok(PipelineResult::Failed) => ExitCode::FAILURE,
        Ok(PipelineResult::Checked { .. }) => ExitCode::SUCCESS,
        Ok(PipelineResult::Tested { .. }) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(3)
        }
    }
}

/// REPL command: interactive mode.
pub fn cmd_repl() -> ExitCode {
    eprintln!("Tungsten {} — Interactive Mode", env!("CARGO_PKG_VERSION"));
    eprintln!("Type expressions to evaluate, or :help for commands.");
    eprintln!();

    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("tg> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }

        let line = line.trim();

        if line.starts_with(':') {
            match line {
                ":quit" | ":q" | ":exit" => break,
                ":help" | ":h" | ":?" => {
                    println!("Commands:");
                    println!("  :help, :h, :?    Show this help");
                    println!("  :quit, :q        Exit the REPL");
                    println!();
                    println!("Enter expressions to evaluate them.");
                }
                _ => {
                    eprintln!("Unknown command: {line}");
                    eprintln!("Type :help for available commands.");
                }
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        match driver::eval_expr(line, false, 20) {
            Ok(PipelineResult::Evaluated { value, ty }) => {
                let value_str = driver::format_value(&value);
                let ty_str = driver::format_type(&ty);
                println!("{value_str} : {ty_str}");
            }
            Ok(PipelineResult::Failed) => {}
            Ok(PipelineResult::Checked { .. }) => {}
            Ok(PipelineResult::Tested { .. }) => {}
            Err(e) => {
                eprintln!("error: {e}");
            }
        }
    }

    ExitCode::SUCCESS
}
