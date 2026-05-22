//! `tungsten diff-ir` — structural LLVM IR comparison.
//!
//! Compares two LLVM IR files structurally (type definitions + function signatures),
//! ignoring SSA register names and metadata noise.
//!
//! Phase: Deferred (T5, ADR 16.4.26a). Stub implementation.
//! See ADR 16.4.26b for full design.

mod normalize;
pub(crate) mod parser;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::ExitCode;

/// Entry point for `tungsten diff-ir`.
pub fn cmd_diff_ir(
    file_a: &Path,
    file_b: &Path,
    types_only: bool,
    signatures_only: bool,
    json: bool,
) -> ExitCode {
    // Validate inputs
    if !file_a.exists() {
        eprintln!("error: file not found: {}", file_a.display());
        return ExitCode::FAILURE;
    }
    if !file_b.exists() {
        eprintln!("error: file not found: {}", file_b.display());
        return ExitCode::FAILURE;
    }

    let ir_a = match std::fs::read_to_string(file_a) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: reading {}: {}", file_a.display(), e);
            return ExitCode::FAILURE;
        }
    };
    let ir_b = match std::fs::read_to_string(file_b) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: reading {}: {}", file_b.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let defs_a = parser::parse_ir_defs(&ir_a);
    let defs_b = parser::parse_ir_defs(&ir_b);

    let diffs = diff_defs(&defs_a, &defs_b, types_only, signatures_only);

    if json {
        print_json_diffs(&diffs);
    } else {
        print_text_diffs(&diffs, file_a, file_b);
    }

    if diffs.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A single difference between two IR files.
#[derive(Debug)]
pub(crate) struct IrDiff {
    pub kind: DiffKind,
    pub name: String,
    pub detail: String,
}

#[derive(Debug)]
pub(crate) enum DiffKind {
    TypeAdded,
    TypeRemoved,
    TypeChanged,
    FuncAdded,
    FuncRemoved,
    FuncChanged,
}

/// Compare parsed IR definitions.
fn diff_defs(
    a: &parser::IrDefs,
    b: &parser::IrDefs,
    types_only: bool,
    signatures_only: bool,
) -> Vec<IrDiff> {
    let mut diffs = Vec::new();

    if !signatures_only {
        // Compare type definitions
        for (name, def_a) in &a.types {
            match b.types.get(name) {
                None => diffs.push(IrDiff {
                    kind: DiffKind::TypeRemoved,
                    name: name.clone(),
                    detail: def_a.clone(),
                }),
                Some(def_b)
                    if normalize::normalize_type(def_a) != normalize::normalize_type(def_b) =>
                {
                    diffs.push(IrDiff {
                        kind: DiffKind::TypeChanged,
                        name: name.clone(),
                        detail: format!("- {def_a}\n+ {def_b}"),
                    });
                }
                _ => {}
            }
        }
        for (name, def_b) in &b.types {
            if !a.types.contains_key(name) {
                diffs.push(IrDiff {
                    kind: DiffKind::TypeAdded,
                    name: name.clone(),
                    detail: def_b.clone(),
                });
            }
        }
    }

    if !types_only {
        // Compare function signatures
        for (name, sig_a) in &a.functions {
            match b.functions.get(name) {
                None => diffs.push(IrDiff {
                    kind: DiffKind::FuncRemoved,
                    name: name.clone(),
                    detail: sig_a.clone(),
                }),
                Some(sig_b)
                    if normalize::normalize_signature(sig_a)
                        != normalize::normalize_signature(sig_b) =>
                {
                    diffs.push(IrDiff {
                        kind: DiffKind::FuncChanged,
                        name: name.clone(),
                        detail: format!("- {sig_a}\n+ {sig_b}"),
                    });
                }
                _ => {}
            }
        }
        for (name, sig_b) in &b.functions {
            if !a.functions.contains_key(name) {
                diffs.push(IrDiff {
                    kind: DiffKind::FuncAdded,
                    name: name.clone(),
                    detail: sig_b.clone(),
                });
            }
        }
    }

    diffs
}

fn print_text_diffs(diffs: &[IrDiff], file_a: &Path, file_b: &Path) {
    if diffs.is_empty() {
        println!("No structural differences.");
        return;
    }

    println!(
        "Structural IR diff: {} vs {}",
        file_a.display(),
        file_b.display()
    );
    println!("{} difference(s):", diffs.len());

    for d in diffs {
        let kind_label = match d.kind {
            DiffKind::TypeAdded => "type added",
            DiffKind::TypeRemoved => "type removed",
            DiffKind::TypeChanged => "type changed",
            DiffKind::FuncAdded => "func added",
            DiffKind::FuncRemoved => "func removed",
            DiffKind::FuncChanged => "func changed",
        };
        println!("  [{}] {}", kind_label, d.name);
        for line in d.detail.lines() {
            println!("    {line}");
        }
    }
}

fn print_json_diffs(diffs: &[IrDiff]) {
    println!("[");
    for (i, d) in diffs.iter().enumerate() {
        let comma = if i + 1 < diffs.len() { "," } else { "" };
        let kind = match d.kind {
            DiffKind::TypeAdded => "type_added",
            DiffKind::TypeRemoved => "type_removed",
            DiffKind::TypeChanged => "type_changed",
            DiffKind::FuncAdded => "func_added",
            DiffKind::FuncRemoved => "func_removed",
            DiffKind::FuncChanged => "func_changed",
        };
        println!(
            "  {{ \"kind\": \"{}\", \"name\": \"{}\", \"detail\": \"{}\" }}{}",
            kind,
            d.name,
            d.detail
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n"),
            comma
        );
    }
    println!("]");
}
