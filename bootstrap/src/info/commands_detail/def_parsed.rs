//! `tungsten info def --no-elaborate`: surface-level definition info from parser AST (cost 2).

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::ast;
use tungsten_bootstrap::parser;

/// Show definition info from parsed AST only, without elaboration.
///
/// This is much cheaper than `cmd_info_def` (cost 2 vs cost 3) because it
/// only parses the file without running the elaboration pipeline.
pub fn cmd_info_def_parsed(name: &str, file: &PathBuf) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };

    let (source_file, errors) = parser::parse(&source);
    if !errors.is_empty() {
        eprintln!("Parse errors in {}:", file.display());
        for e in &errors {
            eprintln!("  {e}");
        }
    }

    // Search for matching function definition
    for item in &source_file.items {
        if let ast::Item::Function(func) = item {
            if func.name.name == name {
                return print_parsed_function(func);
            }
        }
    }

    // Not found — list available definitions
    eprintln!("Definition not found: {name}");
    let mut available: Vec<&str> = source_file
        .items
        .iter()
        .filter_map(|item| match item {
            ast::Item::Function(f) => Some(f.name.name.as_str()),
            _ => None,
        })
        .collect();
    available.sort_unstable();
    if available.len() > 20 {
        eprintln!(
            "Available functions ({} total): {}, ...",
            available.len(),
            available[..20].join(", ")
        );
    } else {
        eprintln!("Available functions: {}", available.join(", "));
    }
    ExitCode::FAILURE
}

fn print_parsed_function(func: &ast::FunctionDef) -> ExitCode {
    println!("Definition: {} (parsed, not elaborated)", func.name.name);
    println!("{}", "═".repeat(12 + func.name.name.len() + 25));
    println!();

    // Type parameters
    if !func.type_params.is_empty() {
        let params: Vec<&str> = func
            .type_params
            .iter()
            .map(|p| p.name.name.as_str())
            .collect();
        println!("Type params: {}", params.join(", "));
    }

    // Parameters
    if func.params.is_empty() {
        println!("Parameters: (none)");
    } else {
        println!("Parameters:");
        for param in &func.params {
            println!("  {:?}: {:?}", param.pattern, param.ty);
        }
    }

    // Return type
    if let Some(ref ret) = func.return_type {
        println!("Return type: {ret:?}");
    }

    // Body (abbreviated)
    println!();
    println!("Body: {:?}", func.body);

    ExitCode::SUCCESS
}
