//! `tungsten info type visibility <name> <file>` — show effective visibility of members.
//!
//! Displays the parent type visibility and per-member (constructor or field)
//! effective visibility, including whether each member inherits or overrides.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_type_short;
use tungsten_bootstrap::ast::Visibility;

/// Entry point for `tungsten info type visibility <name> <file>`.
pub fn cmd_info_type_visibility(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let parent_vis = project.type_visibilities.get(name).copied();

    // Try ADT first, then record
    if let Some((params, constructors)) = project.adt_types.get(name) {
        print_adt_visibility(name, params, constructors, parent_vis);
        ExitCode::SUCCESS
    } else if let Some(fields) = project.record_types.get(name) {
        let field_vis = project.record_field_visibilities.get(name);
        print_record_visibility(name, fields, parent_vis, field_vis);
        ExitCode::SUCCESS
    } else {
        eprintln!("Type not found: {name}");
        let mut available: Vec<&str> = project
            .adt_types
            .keys()
            .chain(project.record_types.keys())
            .map(std::string::String::as_str)
            .collect();
        available.sort_unstable();
        available.dedup();
        if available.is_empty() {
            eprintln!("No types found in this project.");
        } else {
            eprintln!("Available types: {}", available.join(", "));
        }
        ExitCode::FAILURE
    }
}

fn vis_name(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "pub",
        Visibility::Crate => "pub(crate)",
        Visibility::Private => "private",
    }
}

fn print_adt_visibility(
    name: &str,
    params: &[String],
    constructors: &[tungsten_bootstrap::elaborate::Constructor],
    parent_vis: Option<Visibility>,
) {
    let type_display = if params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, params.join(", "))
    };

    let parent = parent_vis.unwrap_or(Visibility::Private);
    println!("Type: {type_display}");
    println!("Declared visibility: {}", vis_name(parent));
    println!("{}", "─".repeat(40));

    if constructors.is_empty() {
        println!("  (no constructors)");
        return;
    }

    for ctor in constructors {
        let effective = ctor.visibility.unwrap_or(parent);
        let source = if ctor.visibility.is_some() {
            "explicit"
        } else {
            "inherited"
        };
        let fields_display = if ctor.fields.is_empty() {
            String::new()
        } else {
            let field_strs: Vec<String> = ctor.fields.iter().map(format_type_short).collect();
            format!("({})", field_strs.join(", "))
        };
        println!(
            "  {} {}{} — {} ({})",
            vis_name(effective),
            ctor.name,
            fields_display,
            vis_name(effective),
            source,
        );
    }
}

fn print_record_visibility(
    name: &str,
    fields: &[(String, tungsten_core::Type)],
    parent_vis: Option<Visibility>,
    field_vis: Option<&Vec<Option<Visibility>>>,
) {
    let parent = parent_vis.unwrap_or(Visibility::Private);
    println!("Type: {name}");
    println!("Declared visibility: {}", vis_name(parent));
    println!("{}", "─".repeat(40));

    if fields.is_empty() {
        println!("  (no fields)");
        return;
    }

    for (i, (field_name, field_type)) in fields.iter().enumerate() {
        let explicit_vis = field_vis.and_then(|fv| fv.get(i).copied()).flatten();
        let effective = explicit_vis.unwrap_or(parent);
        let source = if explicit_vis.is_some() {
            "explicit"
        } else {
            "inherited"
        };
        println!(
            "  {} {}: {} — {} ({})",
            vis_name(effective),
            field_name,
            format_type_short(field_type),
            vis_name(effective),
            source,
        );
    }
}
