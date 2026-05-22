//! `tungsten doctor check constructor-counts` — validate constructor-list integrity (ADR 7.5.26e).
//!
//! For each ADT, checks that the constructor list satisfies five invariants:
//! 1. Entry count equals declared variant count
//! 2. Constructor indices are unique
//! 3. Constructor indices are contiguous from 0..variant_count-1
//! 4. Constructor names are unique within the ADT
//! 5. Every constructor entry references the expected parent type

mod validator;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

pub use validator::{
    validate_constructors, validate_constructors_with_expected, ConstructorValidationResult,
    ConstructorViolation,
};

/// Entry point for `tungsten doctor check constructor-counts <file>`.
pub fn cmd_check_constructor_counts(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    json: bool,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    if project.adt_types.is_empty() {
        if json {
            println!("{{\"adts\": [], \"status\": \"pass\"}}");
        } else {
            println!("No ADTs found.");
        }
        return ExitCode::SUCCESS;
    }

    let mut adt_names: Vec<&String> = project.adt_types.keys().collect();
    adt_names.sort();

    let mut any_failed = false;

    if json {
        print!("{{\"adts\": [");
    }

    for (i, name) in adt_names.iter().enumerate() {
        let (_, constructors) = &project.adt_types[*name];
        let result = validate_constructors(name, constructors);

        if !result.is_ok() {
            any_failed = true;
        }

        if json {
            if i > 0 {
                print!(", ");
            }
            print_json_result(name, &result);
        } else {
            print_text_result(name, &result, verbose);
        }
    }

    if json {
        println!(
            "], \"status\": \"{}\"}}",
            if any_failed { "fail" } else { "pass" }
        );
    }

    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_text_result(name: &str, result: &ConstructorValidationResult, verbose: bool) {
    if result.is_ok() {
        println!(
            "✓ {}: {} entries, {} variants — OK",
            name, result.actual_count, result.expected_count
        );
    } else {
        println!(
            "✗ {}: {} entries, {} variants — FAILED",
            name, result.actual_count, result.expected_count
        );
        for violation in &result.violations {
            println!("    {}", format_violation(violation));
        }
    }

    if verbose {
        for entry in &result.entries {
            println!(
                "    {} — index={}, arity={}, fields=[{}]",
                entry.name, entry.index, entry.arity, entry.field_types_display
            );
        }
    }
}

fn print_json_result(name: &str, result: &ConstructorValidationResult) {
    let violations: Vec<String> = result
        .violations
        .iter()
        .map(|v| format!("\"{}\"", format_violation(v).replace('"', "\\\"")))
        .collect();
    print!(
        "{{\"name\": \"{}\", \"expected\": {}, \"actual\": {}, \"ok\": {}, \"violations\": [{}]}}",
        name,
        result.expected_count,
        result.actual_count,
        result.is_ok(),
        violations.join(", ")
    );
}

fn format_violation(v: &ConstructorViolation) -> String {
    match v {
        ConstructorViolation::CountMismatch { expected, actual } => {
            format!("count mismatch: expected {expected}, got {actual}")
        }
        ConstructorViolation::DuplicateIndex { index, count } => {
            format!("duplicate index {index} (appears {count} times)")
        }
        ConstructorViolation::NonContiguousIndices { missing } => {
            format!("non-contiguous indices: missing {:?}", missing)
        }
        ConstructorViolation::DuplicateName { name, count } => {
            format!("duplicate name \"{name}\" (appears {count} times)")
        }
        ConstructorViolation::WrongParentType {
            constructor,
            expected,
            actual,
        } => {
            format!(
                "constructor \"{constructor}\" has wrong parent type: expected \"{expected}\", got \"{actual}\""
            )
        }
    }
}
