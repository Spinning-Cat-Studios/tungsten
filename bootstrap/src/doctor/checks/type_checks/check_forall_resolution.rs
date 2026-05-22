//! `tungsten doctor check type forall-resolution <file>` — detect inner foralls
//! that would survive into `extract_type_arg_from_match`.
//!
//! After full elaboration, checks each function's inferred type for positions
//! where a Forall type is embedded inside a Product or Sum — these are the
//! positions that cause `extract_type_arg_from_match` to return ExtractMismatch
//! if `resolve_inner_foralls` is not called first.
//!
//! This is a defence-in-depth check: the fix in ADR 21.5.26b ensures
//! `resolve_inner_foralls` is always called before extraction, but this check
//! catches any code path that might bypass that call.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use tungsten_core::types::Type;

/// A position where a Forall is embedded inside a structural type.
#[derive(Debug)]
struct ForallPosition {
    /// The definition name containing this type
    def_name: String,
    /// Human-readable path to the Forall (e.g., "Sum.left > Product.left")
    path: String,
    /// The forall variable name
    var_name: String,
}

/// Entry point for `tungsten doctor check type forall-resolution <file>`.
pub fn cmd_check_forall_resolution(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let positions = collect_inner_foralls(&project);

    if positions.is_empty() {
        if verbose {
            eprintln!("✓ No inner foralls found in structural type positions");
        }
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "⚠ Found {} inner forall(s) in structural positions:",
            positions.len()
        );
        eprintln!();
        for pos in &positions {
            eprintln!(
                "  {} — Forall({}) at {}",
                pos.def_name, pos.var_name, pos.path
            );
        }
        eprintln!();
        eprintln!("These positions require resolve_inner_foralls() before extraction.");
        eprintln!("If the code calls extract_type_arg_from_match on these types");
        eprintln!("without resolving first, it will return ExtractMismatch (ADR 21.5.26b).");
        // This is informational, not a hard failure — the types are valid,
        // they just need care at call sites.
        ExitCode::SUCCESS
    }
}

/// Scan all value definitions for inner foralls in structural positions.
fn collect_inner_foralls(project: &driver::ProjectOutput) -> Vec<ForallPosition> {
    let mut positions = Vec::new();

    for def in &project.defs {
        scan_type_for_inner_foralls(&def.ty, &def.name, "", &mut positions);
    }

    positions.sort_by(|a, b| a.def_name.cmp(&b.def_name));
    positions
}

/// Recursively scan a type for Forall nested inside Sum/Product/Arrow.
fn scan_type_for_inner_foralls(
    ty: &Type,
    def_name: &str,
    path: &str,
    out: &mut Vec<ForallPosition>,
) {
    match ty {
        // Forall at the TOP level is fine — it's the normal polymorphic case.
        // We only care about Forall nested inside structural types.
        Type::Forall(var, body) => {
            // Don't flag the top-level Forall, but recurse into its body
            // to find nested structural types that might contain inner foralls.
            let new_path = if path.is_empty() {
                format!("Forall({var})")
            } else {
                format!("{path} > Forall({var})")
            };
            scan_type_for_inner_foralls(body, def_name, &new_path, out);
        }
        Type::Sum(left, right) => {
            let lp = if path.is_empty() {
                "Sum.left".to_string()
            } else {
                format!("{path} > Sum.left")
            };
            let rp = if path.is_empty() {
                "Sum.right".to_string()
            } else {
                format!("{path} > Sum.right")
            };
            check_and_recurse(left, def_name, &lp, out);
            check_and_recurse(right, def_name, &rp, out);
        }
        Type::Product(left, right) => {
            let lp = if path.is_empty() {
                "Product.left".to_string()
            } else {
                format!("{path} > Product.left")
            };
            let rp = if path.is_empty() {
                "Product.right".to_string()
            } else {
                format!("{path} > Product.right")
            };
            check_and_recurse(left, def_name, &lp, out);
            check_and_recurse(right, def_name, &rp, out);
        }
        Type::Arrow(domain, codomain) => {
            let dp = if path.is_empty() {
                "Arrow.domain".to_string()
            } else {
                format!("{path} > Arrow.domain")
            };
            let cp = if path.is_empty() {
                "Arrow.codomain".to_string()
            } else {
                format!("{path} > Arrow.codomain")
            };
            check_and_recurse(domain, def_name, &dp, out);
            check_and_recurse(codomain, def_name, &cp, out);
        }
        Type::Mu(_var, body) => {
            let mp = if path.is_empty() {
                "Mu.body".to_string()
            } else {
                format!("{path} > Mu.body")
            };
            scan_type_for_inner_foralls(body, def_name, &mp, out);
        }
        // TyVar, Unit, Nat, etc. — leaf types, nothing to check.
        _ => {}
    }
}

/// If the type at this position IS a Forall, flag it. Then recurse regardless.
fn check_and_recurse(ty: &Type, def_name: &str, path: &str, out: &mut Vec<ForallPosition>) {
    if let Type::Forall(var, _) = ty {
        out.push(ForallPosition {
            def_name: def_name.to_string(),
            path: path.to_string(),
            var_name: var.clone(),
        });
    }
    scan_type_for_inner_foralls(ty, def_name, path, out);
}
