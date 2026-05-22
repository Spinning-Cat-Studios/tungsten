//! `tungsten info field-type` — show stored and resolved types for record/ADT fields.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_semantic_type;
/// Show stored and resolved types for a record or ADT field.
///
/// Field path formats:
/// - `Record.field` — look up a record field
/// - `ADT.Constructor.N` — look up field N of an ADT constructor
pub fn cmd_info_field_type(
    field_path: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let parts: Vec<&str> = field_path.split('.').collect();

    match parts.len() {
        2 => lookup_two_part_field(field_path, parts[0], parts[1], &project),
        3 => lookup_three_part_field(field_path, parts[0], parts[1], parts[2], &project),
        _ => {
            eprintln!("Invalid field path: {field_path}");
            eprintln!("Expected: Record.field or ADT.Constructor.field_index");
            ExitCode::FAILURE
        }
    }
}

/// Handle `Type.field` or `ADT.Constructor` field lookup.
fn lookup_two_part_field(
    field_path: &str,
    type_name: &str,
    field_name: &str,
    project: &tungsten_bootstrap::driver::ProjectOutput,
) -> ExitCode {
    // Try record types first
    if let Some(fields) = project.record_types.get(type_name) {
        if let Some((_, field_ty)) = fields.iter().find(|(n, _)| n == field_name) {
            println!("Field: {field_path}");
            println!("{}", "═".repeat(7 + field_path.len()));
            println!();
            println!("  stored:   {field_ty}");
            if let Some(display) = format_semantic_type(field_ty, &project.type_provenance) {
                println!("  display:  {display}");
            }
            return ExitCode::SUCCESS;
        }
        eprintln!("Field '{field_name}' not found on record '{type_name}'");
        let available: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        eprintln!("Available fields: {}", available.join(", "));
        return ExitCode::FAILURE;
    }

    // Try ADT constructor (2-part: ADT.Constructor — show all fields)
    if let Some((_, constructors)) = project.adt_types.get(type_name) {
        if let Some(ctor) = constructors.iter().find(|c| c.name == field_name) {
            println!("Constructor: {type_name}.{field_name}");
            println!("{}", "═".repeat(14 + type_name.len() + field_name.len()));
            println!();
            if ctor.fields.is_empty() {
                println!("  (no fields)");
            } else {
                for (i, field_ty) in ctor.fields.iter().enumerate() {
                    println!("  [{i}] stored:   {field_ty}");
                    if let Some(display) = format_semantic_type(field_ty, &project.type_provenance)
                    {
                        println!("      display:  {display}");
                    }
                }
            }
            return ExitCode::SUCCESS;
        }
        // ADT exists but constructor not found
        eprintln!("Constructor '{field_name}' not found on ADT '{type_name}'");
        let available: Vec<&str> = constructors.iter().map(|c| c.name.as_str()).collect();
        eprintln!("Available constructors: {}", available.join(", "));
        return ExitCode::FAILURE;
    }

    eprintln!("Type '{type_name}' not found as record or ADT");
    let mut available: Vec<&str> = project
        .record_types
        .keys()
        .map(std::string::String::as_str)
        .collect();
    available.extend(project.adt_types.keys().map(std::string::String::as_str));
    available.sort_unstable();
    eprintln!("Available types: {}", available.join(", "));
    ExitCode::FAILURE
}

/// Handle `ADT.Constructor.field_index` field lookup.
fn lookup_three_part_field(
    field_path: &str,
    adt_name: &str,
    ctor_name: &str,
    field_index: &str,
    project: &tungsten_bootstrap::driver::ProjectOutput,
) -> ExitCode {
    let Some((_, constructors)) = project.adt_types.get(adt_name) else {
        eprintln!("ADT not found: {adt_name}");
        let mut available: Vec<&str> = project
            .adt_types
            .keys()
            .map(std::string::String::as_str)
            .collect();
        available.sort_unstable();
        eprintln!("Available ADTs: {}", available.join(", "));
        return ExitCode::FAILURE;
    };

    let Some(ctor) = constructors.iter().find(|c| c.name == ctor_name) else {
        eprintln!("Constructor '{ctor_name}' not found on ADT '{adt_name}'");
        let available: Vec<&str> = constructors.iter().map(|c| c.name.as_str()).collect();
        eprintln!("Available constructors: {}", available.join(", "));
        return ExitCode::FAILURE;
    };

    let Ok(idx) = field_index.parse::<usize>() else {
        eprintln!("Field index must be a number, got: {field_index}");
        return ExitCode::FAILURE;
    };

    if idx >= ctor.fields.len() {
        eprintln!(
            "Field index {idx} out of range for {adt_name}.{ctor_name} ({} fields)",
            ctor.fields.len()
        );
        return ExitCode::FAILURE;
    }

    let field_ty = &ctor.fields[idx];
    let is_self_ref = matches!(field_ty, tungsten_core::types::Type::TyVar(v) if v == adt_name);

    println!("Field: {field_path}");
    println!("{}", "═".repeat(7 + field_path.len()));
    println!();
    println!("  stored:   {field_ty}");

    if is_self_ref {
        println!("  ↳ bare self-reference (type args discarded during Phase 1c)");
        if let Some(encoded) = project.encoded_types.get(adt_name) {
            let display = format_semantic_type(encoded, &project.type_provenance)
                .unwrap_or_else(|| format!("{encoded}"));
            println!("  resolved: {display}");
        }
    } else if let Some(display) = format_semantic_type(field_ty, &project.type_provenance) {
        println!("  display:  {display}");
    }

    ExitCode::SUCCESS
}
