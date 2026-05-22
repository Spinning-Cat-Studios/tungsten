//! `info constructors` — inspect constructor list entries for a given ADT (ADR 7.5.26e).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::doctor::checks::check_constructor_counts::{
    validate_constructors, ConstructorViolation,
};
use crate::info::elaborate_for_info;

/// Entry point for `tungsten info constructors <type> <file>`.
pub fn cmd_info_constructors(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let Some((params, constructors)) = project.adt_types.get(name) else {
        eprintln!("ADT not found: {name}");
        let mut available: Vec<&str> = project
            .adt_types
            .keys()
            .map(std::string::String::as_str)
            .collect();
        available.sort_unstable();
        eprintln!("Available ADTs: {}", available.join(", "));
        return ExitCode::FAILURE;
    };

    let type_display = if params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, params.join(", "))
    };

    let result = validate_constructors(name, constructors);

    // Header
    println!(
        "Type: {} ({} variants in source)",
        type_display, result.expected_count
    );
    println!("Constructor entries: {}", result.actual_count);

    // Grouped entries
    for (ctor_name, index, count) in &result.grouped {
        let field_display = result
            .entries
            .iter()
            .find(|e| &e.name == ctor_name && e.index == *index)
            .map(|e| e.field_types_display.clone())
            .unwrap_or_default();

        let suffix = if *count > 1 {
            format!("  ×{count}")
        } else {
            String::new()
        };

        println!(
            "  {} — index={}, arity={}, field_types={}{}",
            ctor_name,
            index,
            result
                .entries
                .iter()
                .find(|e| &e.name == ctor_name)
                .map_or(0, |e| e.arity),
            field_display,
            suffix,
        );
    }

    // Validation result
    if !result.is_ok() {
        println!();
        for violation in &result.violations {
            println!("⚠ {}", format_violation(violation));
        }
    }

    ExitCode::SUCCESS
}

fn format_violation(v: &ConstructorViolation) -> String {
    match v {
        ConstructorViolation::CountMismatch { expected, actual } => {
            format!(
                "Duplicates detected: env_count_constructors would return {actual} (expected {expected})"
            )
        }
        ConstructorViolation::DuplicateIndex { index, count } => {
            format!("Duplicate index {index} (appears {count} times)")
        }
        ConstructorViolation::NonContiguousIndices { missing } => {
            format!("Non-contiguous indices: missing {missing:?}")
        }
        ConstructorViolation::DuplicateName { name, count } => {
            format!("Duplicate name \"{name}\" (appears {count} times)")
        }
        ConstructorViolation::WrongParentType {
            constructor,
            expected,
            actual,
        } => {
            format!(
                "Constructor \"{constructor}\" has wrong parent: expected \"{expected}\", got \"{actual}\""
            )
        }
    }
}
