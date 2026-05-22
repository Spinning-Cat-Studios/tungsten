//! Info subcommand implementations for ADT details, definitions, and encodings.

mod constructors;
mod def;
mod def_parsed;
mod diagnostic;
mod error_enrichment;
mod field_type;
mod fold_check;
mod record_fields;
mod try_desugar;
mod visibility;

pub use constructors::cmd_info_constructors;
pub use def::cmd_info_def;
pub use def_parsed::cmd_info_def_parsed;
pub use diagnostic::{cmd_info_mutual_recursion_groups, cmd_info_type_encoding};
pub use error_enrichment::cmd_info_error_enrichment;
pub use field_type::cmd_info_field_type;
pub use record_fields::cmd_info_record_fields;
pub use try_desugar::cmd_info_try_desugar;
pub use visibility::cmd_info_type_visibility;

use std::path::PathBuf;
use std::process::ExitCode;

use super::elaborate_for_info;
use super::helpers::{
    encode_body_description, encode_ctor_fields, format_semantic_type, format_type_short,
};
use fold_check::print_fold_check;
use tungsten_bootstrap::elaborate::Constructor;

// ═══════════════════════════════════════════════════════════════════════
// tungsten info adt
// ═══════════════════════════════════════════════════════════════════════

/// Options for the `info adt` subcommand.
pub struct AdtInfoOptions {
    pub verbose: bool,
    pub max_errors: usize,
    pub show_fields: bool,
    pub check_fold: bool,
}

pub fn cmd_info_adt(name: &str, file: &PathBuf, opts: &AdtInfoOptions) -> ExitCode {
    let AdtInfoOptions {
        verbose,
        max_errors,
        show_fields,
        check_fold,
    } = *opts;
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

    let type_params = if params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, params.join(", "))
    };

    println!("ADT: {type_params}");
    println!("{}", "═".repeat(4 + type_params.len()));
    println!();

    print_adt_constructors(constructors);

    if show_fields {
        print_adt_field_provenance(
            name,
            constructors,
            &project.encoded_types,
            &project.type_provenance,
        );
    }

    print_adt_properties(
        name,
        params,
        constructors,
        &project.type_provenance,
        verbose,
    );

    // Used by (definitions referencing this ADT)
    let used_by: Vec<&str> = project
        .defs
        .iter()
        .filter(|d| {
            let ty_str = format!("{}", d.ty);
            ty_str.contains(name) || ty_str.contains(&format!("α_{name}"))
        })
        .map(|d| d.name.as_str())
        .collect();

    if !used_by.is_empty() {
        println!(
            "Used by: {}",
            if used_by.len() > 5 {
                format!("{}, ... ({} total)", used_by[..5].join(", "), used_by.len())
            } else {
                used_by.join(", ")
            }
        );
    }

    if check_fold {
        return print_fold_check(name, &project);
    }

    ExitCode::SUCCESS
}

/// Print the constructors section of ADT info.
fn print_adt_constructors(constructors: &[Constructor]) {
    println!("Constructors:");
    for (i, ctor) in constructors.iter().enumerate() {
        let fields = if ctor.fields.is_empty() {
            "()".to_string()
        } else {
            let f: Vec<String> = ctor.fields.iter().map(format_type_short).collect();
            format!("({})", f.join(", "))
        };
        let encoded = if ctor.fields.is_empty() {
            "Unit".to_string()
        } else if ctor.fields.len() == 1 {
            format_type_short(&ctor.fields[0])
        } else {
            let parts: Vec<String> = ctor.fields.iter().map(format_type_short).collect();
            format!("Product({})", parts.join(", "))
        };
        println!("  {}: {:<14}{:<24}→ {}", i, ctor.name, fields, encoded);
    }
    println!();
}

/// Print the properties section of ADT info.
fn print_adt_properties(
    name: &str,
    params: &[String],
    constructors: &[Constructor],
    type_provenance: &tungsten_bootstrap::elaborate::TypeProvenance,
    verbose: bool,
) {
    let is_recursive = type_provenance
        .mu_origins
        .values()
        .any(|o| o.adt_name == name);
    let mu_var = format!("α_{name}");

    println!("Properties:");
    if !params.is_empty() {
        println!("  Type parameters: [{}]", params.join(", "));
    }
    println!(
        "  Recursive:       {}",
        if is_recursive { "yes" } else { "no" }
    );

    let strategy = match constructors.len() {
        0 => "Void (0 constructors)".to_string(),
        1 => "single constructor (unwrapped)".to_string(),
        2 => "binary Sum (2 constructors)".to_string(),
        n => format!("flat Adt ({n} constructors)"),
    };
    println!("  Encoding:        {strategy}");
    if is_recursive {
        println!("  Mu binder:       {mu_var}");
    }
    println!();

    // Structural encoding (from provenance if available)
    if let Some(origin) = type_provenance
        .mu_origins
        .values()
        .find(|o| o.adt_name == name)
    {
        let mu_key = type_provenance
            .mu_origins
            .iter()
            .find(|(_, o)| o.adt_name == name)
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        if !mu_key.is_empty() && verbose {
            println!("  Mu binder key:   {mu_key}");
            if !origin.type_args.is_empty() {
                let args: Vec<String> = origin.type_args.iter().map(|a| format!("{a}")).collect();
                println!("  Instantiation:   [{}]", args.join(", "));
            }
        }
    }
}

/// Print field type provenance for each constructor (--show-fields).
///
/// Shows stored form (Phase 1c) vs resolved form (after encoding) for each field.
fn print_adt_field_provenance(
    name: &str,
    constructors: &[Constructor],
    encoded_types: &std::collections::HashMap<String, tungsten_core::types::Type>,
    type_provenance: &tungsten_bootstrap::elaborate::TypeProvenance,
) {
    println!("Field Provenance:");
    for ctor in constructors {
        if ctor.fields.is_empty() {
            println!("  {}()  — no fields", ctor.name);
            continue;
        }
        println!("  {}:", ctor.name);
        for (i, field_ty) in ctor.fields.iter().enumerate() {
            let stored = format!("{field_ty}");
            let is_self_ref = matches!(field_ty, tungsten_core::types::Type::TyVar(v) if v == name);
            let is_at_ref =
                matches!(field_ty, tungsten_core::types::Type::TyVar(v) if v.starts_with('@'));

            println!("    [{i}] stored:   {stored}");

            // Show resolved form from encoded_types if available
            if is_self_ref {
                println!("        ↳ bare self-reference (type args discarded during Phase 1c)");
                println!("          resolved via substitute_recursive_refs in Phase 2");
                if let Some(encoded) = encoded_types.get(name) {
                    let display = format_semantic_type(encoded, type_provenance)
                        .unwrap_or_else(|| format!("{encoded}"));
                    println!("        resolved: {display}");
                }
            } else if is_at_ref {
                println!("        ↳ deferred TyVar (resolved in Phase 1d)");
            } else if let Some(display) = format_semantic_type(field_ty, type_provenance) {
                println!("        resolved: {display}");
            }
        }
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════
// tungsten info encoding
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_info_encoding(name: &str, file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
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

    let is_recursive = project
        .type_provenance
        .mu_origins
        .values()
        .any(|o| o.adt_name == name);
    let mu_var = format!("α_{name}");

    let type_params = if params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, params.join(", "))
    };

    let ctor_count = constructors.len();
    let strategy = match ctor_count {
        0 => "Void (0 constructors)".to_string(),
        1 => "single constructor (unwrapped)".to_string(),
        2 => "binary Sum (2 constructors)".to_string(),
        n => format!("flat Adt ({n} constructors)"),
    };

    println!("Encoding: {type_params}");
    println!("{}", "═".repeat(10 + type_params.len()));
    println!();
    println!("Strategy:     {strategy}");
    println!("Recursive:    {}", if is_recursive { "yes" } else { "no" });
    println!("Constructors: {ctor_count}");
    if is_recursive {
        println!("Mu binder:    {mu_var}");
    }
    println!();

    // Constructor encoding breakdown
    println!("Constructor Layout:");
    for (i, ctor) in constructors.iter().enumerate() {
        let fields_desc = if ctor.fields.is_empty() {
            "(none)".to_string()
        } else {
            ctor.fields
                .iter()
                .map(format_type_short)
                .collect::<Vec<_>>()
                .join(", ")
        };

        let encoded = if ctor.fields.is_empty() {
            "Unit".to_string()
        } else if ctor.fields.len() == 1 {
            format_type_short(&ctor.fields[0])
        } else {
            // Right-nested product
            let parts: Vec<String> = ctor.fields.iter().map(format_type_short).collect();
            parts.join(" × ")
        };

        println!("  [{i}] {}", ctor.name);
        println!("      Fields:  {fields_desc}");
        println!("      Encoded: {encoded}");
        if i < ctor_count - 1 {
            println!();
        }
    }
    println!();

    // Structural encoding
    println!("Structural Encoding:");
    if is_recursive {
        let body = encode_body_description(constructors, &mu_var);
        println!("  μ{mu_var}. {body}");
    } else if ctor_count == 1 {
        let body = encode_ctor_fields(&constructors[0]);
        println!("  {body}");
    } else {
        let body = encode_body_description(constructors, &mu_var);
        println!("  {body}");
    }

    ExitCode::SUCCESS
}
