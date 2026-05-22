//! `tungsten info type record-fields <type> <file>` — show record field layout.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_type_short;
pub fn cmd_info_record_fields(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let Some(fields) = project.record_types.get(name) else {
        eprintln!("Record type not found: {name}");
        let mut available: Vec<&str> = project
            .record_types
            .keys()
            .map(std::string::String::as_str)
            .collect();
        available.sort_unstable();
        if available.is_empty() {
            eprintln!("No record types found in this project.");
        } else {
            eprintln!("Available record types: {}", available.join(", "));
        }
        return ExitCode::FAILURE;
    };

    println!("Record: {name}");
    println!("{}", "═".repeat(8 + name.len()));
    println!();

    println!("Fields ({} total, canonical order):", fields.len());
    for (i, (field_name, field_ty)) in fields.iter().enumerate() {
        let ty_str = format_type_short(field_ty);
        let position = if fields.len() == 1 {
            "base".to_string()
        } else {
            // Build projection chain: fst(snd(snd(base))) etc.
            let mut inner = "base".to_string();
            for _ in 0..i {
                inner = format!("snd({inner})");
            }
            if i < fields.len() - 1 {
                format!("fst({inner})")
            } else {
                inner
            }
        };
        println!("  {i}: {field_name}: {ty_str}");
        if verbose {
            println!("     projection: {position}");
        }
    }

    // Show the product encoding
    if fields.len() > 1 {
        println!();
        let product = fields
            .iter()
            .map(|(_, ty)| format_type_short(ty))
            .collect::<Vec<_>>();
        // Right-nested product
        let mut encoding = product.last().unwrap().clone();
        for ty in product.iter().rev().skip(1) {
            encoding = format!("({ty} × {encoding})");
        }
        println!("Product encoding: {encoding}");
    }

    ExitCode::SUCCESS
}
