//! ABI Layout Dump — `tungsten dump-abi`
//!
//! Inspects LLVM IR files and reports struct layouts, ABI passing decisions
//! (DIRECT vs INDIRECT), and optionally register assignments via `llc`.
//!
//! See ADR 17.4.26b for design rationale.

mod analyze;
mod layout;
mod llc;
mod parse;
mod report;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::ExitCode;

use crate::diff_ir::parser;
use analyze::{analyze_function, suggest_similar_functions};
use layout::TypeLayout;
use llc::run_llc_analysis;
use report::format_report;

/// AAPCS64 threshold: structs ≤ 16 bytes are passed directly in registers.
const AAPCS64_DIRECT_THRESHOLD: u64 = 16;

/// ABI passing mode for a parameter or return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PassingMode {
    /// Passed directly in register(s).
    Direct,
    /// Passed indirectly via pointer.
    Indirect,
}

/// Analyzed ABI info for a single parameter or return value.
#[derive(Debug, Clone)]
pub(crate) struct ParamAbi {
    /// Parameter name (e.g., "arg0", or parsed name if available).
    pub name: String,
    /// The LLVM IR type string.
    pub ty: String,
    /// Computed layout (if the type is a struct/array).
    pub layout: Option<TypeLayout>,
    /// Passing mode.
    pub passing: PassingMode,
}

/// Analyzed ABI info for one function.
#[derive(Debug, Clone)]
pub(crate) struct FunctionAbi {
    /// Function name (without @).
    pub name: String,
    /// Full signature string (retained for diagnostics).
    #[allow(dead_code)]
    pub signature: String,
    /// Parameter ABI info.
    pub params: Vec<ParamAbi>,
    /// Return type ABI info.
    pub ret: ParamAbi,
}

/// Tier 2 register info from llc output (one entry per argument).
#[derive(Debug, Clone)]
pub(crate) struct LlcRegisterInfo {
    pub raw_output: String,
}

/// Entry point for `tungsten dump-abi`.
pub fn cmd_dump_abi(function_name: Option<&str>, file: &Path, all: bool, deep: bool) -> ExitCode {
    // Read the .ll file
    let ir_text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: could not read '{}': {}", file.display(), e);
            return ExitCode::FAILURE;
        }
    };

    // Parse type definitions and function signatures
    let defs = parser::parse_ir_defs(&ir_text);

    if all {
        return cmd_dump_all(&defs, deep, file);
    }

    let fn_name = match function_name {
        Some(name) => name,
        None => {
            eprintln!("error: must specify a function name or use --all");
            return ExitCode::FAILURE;
        }
    };

    // Find the function
    let signature = match defs.functions.get(fn_name) {
        Some(sig) => sig.clone(),
        None => {
            eprintln!(
                "error: function '{}' not found in {}",
                fn_name,
                file.display()
            );
            suggest_similar_functions(fn_name, &defs);
            return ExitCode::FAILURE;
        }
    };

    // Analyze ABI
    let func_abi = match analyze_function(fn_name, &signature, &defs) {
        Ok(abi) => abi,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Tier 2: llc integration (optional)
    let llc_info = if deep { run_llc_analysis(file) } else { None };

    // Format and print report
    format_report(&func_abi, llc_info.as_ref());

    ExitCode::SUCCESS
}

/// Dump ABI for all functions in the file.
fn cmd_dump_all(defs: &parser::IrDefs, deep: bool, file: &Path) -> ExitCode {
    let mut names: Vec<&String> = defs.functions.keys().collect();
    names.sort();

    if names.is_empty() {
        eprintln!("warning: no functions found in {}", file.display());
        return ExitCode::SUCCESS;
    }

    let llc_info = if deep { run_llc_analysis(file) } else { None };

    let mut indirect_count = 0u32;
    let mut warning_count = 0u32;
    let total = names.len();

    for name in &names {
        let sig = &defs.functions[*name];
        match analyze_function(name, sig, defs) {
            Ok(func_abi) => {
                for p in &func_abi.params {
                    if p.passing == PassingMode::Indirect {
                        indirect_count += 1;
                    }
                }
                format_report(&func_abi, llc_info.as_ref());
            }
            Err(e) => {
                eprintln!("warning: {name}: {e}");
                warning_count += 1;
            }
        }
    }

    println!("───────────────────");
    println!("{total} functions, {indirect_count} INDIRECT, {warning_count} warnings");

    ExitCode::SUCCESS
}
