//! Info subcommand implementations for pipeline overview and type listing.
//!
//! ADT details, definition inspection, and encoding explanations are in `commands_detail.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use super::elaborate_for_info;
use super::helpers::format_type_short;

pub use super::commands_detail::{
    cmd_info_adt, cmd_info_constructors, cmd_info_def, cmd_info_def_parsed, cmd_info_encode_order,
    cmd_info_encoding, cmd_info_error_enrichment, cmd_info_field_type, cmd_info_mu_members,
    cmd_info_mutual_recursion_groups, cmd_info_record_fields, cmd_info_try_desugar,
    cmd_info_type_encoding, cmd_info_type_size, cmd_info_type_spine, cmd_info_type_visibility,
    AdtInfoOptions, DefReports,
};

pub(crate) mod pipeline;
pub use pipeline::cmd_info_pipeline;

// Compiled in both configurations so its unit tests run in the default
// (no-codegen) test config; only the codegen-less CLI dispatches to it, hence
// the dead-code allowance under `codegen` (the ADR 5.8.26c retrospective's
// `info/mod.rs:23` precedent, inverted).
#[cfg_attr(feature = "codegen", allow(dead_code))]
mod codegen_unavailable;
#[cfg_attr(feature = "codegen", allow(unused_imports))]
pub use codegen_unavailable::cmd_info_codegen_unavailable;

pub fn cmd_info_types(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let num_records = print_record_types(&project.record_types);
    let num_adts = print_adt_types(&project.adt_types, &project.type_provenance);
    let num_aliases = print_type_aliases(&project.type_aliases);

    // Summary
    println!(
        "Summary: {} records, {} ADTs, {} aliases = {} types total",
        num_records,
        num_adts,
        num_aliases,
        num_records + num_adts + num_aliases
    );

    ExitCode::SUCCESS
}

fn print_record_types(record_types: &tungsten_bootstrap::driver::RecordTypes) -> usize {
    let mut record_names: Vec<&String> = record_types.keys().collect();
    record_names.sort();

    if !record_names.is_empty() {
        println!("Record Types ({}):", record_names.len());
        for name in &record_names {
            let fields = &record_types[*name];
            let fields_str: Vec<String> = fields
                .iter()
                .map(|(fname, fty)| format!("{}: {}", fname, format_type_short(fty)))
                .collect();
            println!("  {:<20}{{ {} }}", name, fields_str.join(", "));
        }
        println!();
    }
    record_names.len()
}

fn print_adt_types(
    adt_types: &tungsten_bootstrap::driver::AdtTypes,
    type_provenance: &tungsten_bootstrap::elaborate::TypeProvenance,
) -> usize {
    let mut adt_names: Vec<&String> = adt_types.keys().collect();
    adt_names.sort();

    if !adt_names.is_empty() {
        println!("ADT Types ({}):", adt_names.len());
        for name in &adt_names {
            let (params, constructors) = &adt_types[*name];
            let is_recursive = type_provenance
                .mu_origins
                .values()
                .any(|o| &o.adt_name == *name);
            let type_params = if params.is_empty() {
                (*name).clone()
            } else {
                format!("{}<{}>", name, params.join(", "))
            };
            let ctors: Vec<String> = constructors
                .iter()
                .map(|c| {
                    if c.fields.is_empty() {
                        c.name.clone()
                    } else {
                        let fields: Vec<String> = c.fields.iter().map(format_type_short).collect();
                        format!("{}({})", c.name, fields.join(", "))
                    }
                })
                .collect();
            let props = if is_recursive {
                format!("[recursive, {} ctors]", constructors.len())
            } else {
                format!("[{} ctors]", constructors.len())
            };
            println!("  {:<20}= {}  {}", type_params, ctors.join(" | "), props);
        }
        println!();
    }
    adt_names.len()
}

fn print_type_aliases(type_aliases: &tungsten_bootstrap::driver::TypeAliases) -> usize {
    let mut alias_names: Vec<&String> = type_aliases.keys().collect();
    alias_names.sort();

    if !alias_names.is_empty() {
        println!("Type Aliases ({}):", alias_names.len());
        for name in &alias_names {
            let (params, target) = &type_aliases[*name];
            let display_name = if params.is_empty() {
                (*name).clone()
            } else {
                format!("{}<{}>", name, params.join(", "))
            };
            println!("  {:<20}= {}", display_name, format_type_short(target));
        }
        println!();
    }
    alias_names.len()
}

// ═══════════════════════════════════════════════════════════════════════
// tungsten info symbols (requires codegen feature)
// ═══════════════════════════════════════════════════════════════════════

/// Map a definition name to its LLVM-level name.
#[cfg(feature = "codegen")]
fn def_llvm_name(name: &str) -> String {
    if name == "main" {
        "tungsten_main".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(feature = "codegen")]
pub fn cmd_info_symbols(
    file: &PathBuf,
    by_function: Option<&str>,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    use tungsten_bootstrap::driver;
    use tungsten_codegen::inkwell::context::Context as LlvmContext;
    use tungsten_codegen::CodeGen;

    // Elaborate the project
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Run codegen to build symbol table
    let llvm_context = LlvmContext::create();
    let module_name = file.file_stem().unwrap_or_default().to_string_lossy();
    let mut codegen = CodeGen::new(&llvm_context, &module_name);

    // Register types
    codegen.register_record_types(project.record_types);
    codegen.register_adt_types(crate::compile::convert_adt_types_for_codegen(
        project.adt_types,
    ));

    // Declare, register terms, and compile all definitions
    populate_codegen_defs(&mut codegen, &project.defs, verbose);

    // Print the symbol table
    let symbols = codegen.symbol_map();
    match by_function {
        // Rendering lives in `info/codegen/symbols.rs` — see its module doc for
        // why codegen-gated code must stay inside that directory.
        Some(name) => crate::info::codegen::symbols::print_function_symbol_set(name, symbols),
        None => print_symbol_table(symbols),
    }

    ExitCode::SUCCESS
}

/// `tungsten info type lowering <name> <file>` (ADR 12.7.26c P4/D5).
///
/// Prints the named type's LLVM layout via each applicable lowering route,
/// flagging divergence. Shares the route-lowering probe with
/// `doctor check type lowering-consistency`, so the two cannot disagree.
#[cfg(feature = "codegen")]
pub fn cmd_info_type_lowering(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    use tungsten_bootstrap::doctor::lowering_probe::{
        build_project_lowering, layouts_diverge, type_route_layouts,
    };
    use tungsten_bootstrap::driver;
    use tungsten_codegen::inkwell::context::Context as LlvmContext;

    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let Some((_, (params, _))) = project.adt_types.iter().find(|(n, _)| n.as_str() == name) else {
        eprintln!("Type '{name}' is not a project ADT (records/aliases have one fixed layout).");
        return ExitCode::FAILURE;
    };
    let params = params.clone();

    let context = LlvmContext::create();
    let mut lowering = build_project_lowering(&context, &project);

    if lowering.is_recursive_adt(name) {
        println!("{name}: recursive ADT — lowers uniformly to `ptr` at the value level (no route divergence possible).");
        return ExitCode::SUCCESS;
    }

    let layouts = type_route_layouts(&mut lowering, name, &params);
    if layouts.is_empty() {
        eprintln!("No applicable lowering routes for '{name}'.");
        return ExitCode::FAILURE;
    }

    let args = if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", vec!["String"; params.len()].join(", "))
    };
    println!("lowering routes for `{name}{args}`:");
    for rl in &layouts {
        println!("  {:<10} {}   ← {}", rl.route, rl.layout, rl.label);
    }
    if layouts_diverge(&layouts) {
        println!("\n  DIVERGENT — routes disagree; run `tungsten doctor check type lowering-consistency {}`", file.display());
        ExitCode::FAILURE
    } else {
        println!("\n  all routes agree.");
        ExitCode::SUCCESS
    }
}

/// Declare functions, register term definitions, and compile all defs into codegen.
#[cfg(feature = "codegen")]
fn populate_codegen_defs(
    codegen: &mut tungsten_codegen::CodeGen,
    defs: &[tungsten_bootstrap::driver::CoreDef],
    verbose: bool,
) {
    // Pass 1: Declare all functions
    for def in defs {
        let llvm_name = def_llvm_name(&def.name);
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            codegen.register_def_type(&llvm_name, &def.ty);
        } else if let Err(e) = codegen.declare_def(&llvm_name, &def.ty) {
            eprintln!("warning: declaration failed for '{}': {}", def.name, e);
            continue;
        }
    }

    // Pass 2: Register term definitions for monomorphization
    for def in defs {
        let llvm_name = def_llvm_name(&def.name);
        codegen.register_term_def(&llvm_name, def.term.term.clone());
    }

    // Pass 3: Compile all definitions to build the symbol map
    for def in defs {
        let llvm_name = def_llvm_name(&def.name);
        if matches!(&def.ty, tungsten_core::types::Type::Forall(_, _)) {
            continue;
        }
        if let Err(e) = codegen.compile_def_with_span(
            &llvm_name,
            &def.term.term,
            &def.ty,
            def.term.span.map(|s| s.start),
        ) {
            if verbose {
                eprintln!("warning: codegen failed for '{}': {}", def.name, e);
            }
        }
    }
}

/// Print a formatted symbol table.
#[cfg(feature = "codegen")]
fn print_symbol_table(symbols: &[tungsten_codegen::SymbolEntry]) {
    if symbols.is_empty() {
        println!("No lambda functions found.");
        return;
    }

    println!("{:<20} {:<30} {}", "IR Name", "Source Name", "File:Line");
    println!("{}", "─".repeat(70));

    for entry in symbols {
        let source = entry.source_name.as_deref().unwrap_or("(anonymous)");
        let location = match (&entry.file, entry.line) {
            (Some(f), Some(l)) => format!("{}:{}", f, l),
            (Some(f), None) => f.clone(),
            _ => String::new(),
        };
        println!("{:<20} {:<30} {}", entry.ir_name, source, location);
    }

    println!();
    println!(
        "Total: {} lambda functions ({} named, {} anonymous)",
        symbols.len(),
        symbols.iter().filter(|e| e.source_name.is_some()).count(),
        symbols.iter().filter(|e| e.source_name.is_none()).count(),
    );
}
