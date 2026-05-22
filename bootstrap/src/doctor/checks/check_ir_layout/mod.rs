//! Store/load type-width consistency check for LLVM IR (ADR 21.4.26b).
//!
//! Parses an emitted `.ll` file and reports store/load instructions where
//! the value type width disagrees with the pointer target type.
//!
//! This is a heuristic check — legitimate LLVM constructs (bitcasts, memcpy
//! lowering, opaque pointers) may produce apparent mismatches. False positives
//! are reported as warnings, not errors.

mod parsing;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_advanced;

use std::path::PathBuf;
use std::process::ExitCode;

use parsing::find_store_load_mismatches;
pub(crate) use parsing::StoreMismatch;

/// Run the IR layout check on a `.ll` file.
pub fn cmd_check_ir_layout(file: &PathBuf, json: bool) -> ExitCode {
    let ir = match std::fs::read_to_string(file) {
        Ok(contents) => contents,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };

    let mismatches = find_store_load_mismatches(&ir);

    if json {
        print_json(file, &mismatches);
    } else {
        print_human(file, &mismatches);
    }

    if mismatches.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_human(file: &PathBuf, mismatches: &[StoreMismatch]) {
    println!("Checking store/load consistency in {}...", file.display());

    if mismatches.is_empty() {
        println!("  ✓ No type-width mismatches found.");
        return;
    }

    for m in mismatches {
        println!(
            "  ✗ {} (line {}):",
            m.function.as_deref().unwrap_or("<unknown>"),
            m.line_number
        );
        println!("    {}", m.instruction.trim());
        println!(
            "    Value type size: {} bytes, pointer target size: {} bytes",
            m.value_size, m.pointer_size
        );
    }
    println!();
    println!(
        "{} inconsistenc{} found.",
        mismatches.len(),
        if mismatches.len() == 1 { "y" } else { "ies" }
    );
}

fn print_json(file: &PathBuf, mismatches: &[StoreMismatch]) {
    println!("{}", format_json(file, mismatches));
}

/// Format mismatches as a JSON string.
///
/// Extracted from `print_json` so it can be unit-tested without capturing stdout.
fn format_json(file: &PathBuf, mismatches: &[StoreMismatch]) -> String {
    let entries: Vec<String> = mismatches
        .iter()
        .map(|m| {
            let func = match &m.function {
                Some(f) => format!("\"{}\"", f),
                None => "null".to_string(),
            };
            let escaped_instruction = m.instruction.trim()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            format!(
                "    {{\"line\": {}, \"function\": {}, \"value_size\": {}, \"pointer_size\": {}, \"instruction\": \"{}\"}}",
                m.line_number, func, m.value_size, m.pointer_size,
                escaped_instruction
            )
        })
        .collect();

    let status = if mismatches.is_empty() {
        "pass"
    } else {
        "fail"
    };
    format!(
        "{{\"file\": \"{}\", \"mismatches\": [\n{}\n  ], \"status\": \"{}\"}}",
        file.display(),
        entries.join(",\n"),
        status
    )
}
