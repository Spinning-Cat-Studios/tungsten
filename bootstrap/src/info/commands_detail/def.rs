//! `tungsten info def`: definition type signature and Core IR inspection.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::info::elaborate_for_info;
use crate::info::helpers::format_semantic_type;
/// Show definition type signature, Core IR, free `TyVars`, and ADT references.
pub fn cmd_info_def(name: &str, file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let Some(def) = project.defs.iter().find(|d| d.name == name) else {
        eprintln!("Definition not found: {name}");
        let mut available: Vec<&str> = project.defs.iter().map(|d| d.name.as_str()).collect();
        available.sort_unstable();
        if available.len() > 20 {
            eprintln!(
                "Available definitions ({} total): {}, ...",
                available.len(),
                available[..20].join(", ")
            );
        } else {
            eprintln!("Available definitions: {}", available.join(", "));
        }
        return ExitCode::FAILURE;
    };

    let structural = format!("{}", def.ty);
    let semantic = format_semantic_type(&def.ty, &project.type_provenance);

    println!("Definition: {name}");
    println!("{}", "═".repeat(12 + name.len()));
    println!();

    if let Some(ref sem) = semantic {
        println!("Type (semantic):    {sem}");
        println!("Type (structural):  {structural}");
    } else {
        println!("Type: {structural}");
    }
    println!();

    println!("Core IR:");
    println!("  {}", def.term);
    println!();

    let free = def.term.free_type_vars();
    let genuine: std::collections::HashSet<_> =
        free.into_iter().filter(|v| !v.starts_with('@')).collect();
    if genuine.is_empty() {
        println!("Free TyVars: ∅");
    } else {
        println!("Free TyVars: {genuine:?}");
    }

    // ADT references
    let ty_str = format!("{}", def.ty);
    let adt_refs: Vec<&str> = project
        .type_provenance
        .mu_origins
        .values()
        .filter(|o| ty_str.contains(&format!("α_{}", o.adt_name)))
        .map(|o| o.adt_name.as_str())
        .collect();
    if !adt_refs.is_empty() {
        let mut unique_refs: Vec<&str> = adt_refs;
        unique_refs.sort_unstable();
        unique_refs.dedup();
        println!("ADT references: {}", unique_refs.join(", "));
    }

    ExitCode::SUCCESS
}
