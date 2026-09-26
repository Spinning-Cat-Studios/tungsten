//! `tungsten doctor check type integrity constructor-stubs` — detect stale constructor metadata.
//!
//! After full elaboration, checks that:
//! 1. No ADT's encoded type is a raw `TyVar` (would indicate stale Stub Registration stub)
//! 2. No constructor field type is a raw `TyVar` matching an ADT name
//!
//! These conditions indicate that Signature Collection export deduplication or type
//! encoding failed, leaving placeholder data that causes downstream match
//! dispatch failures (E0999).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use tungsten_core::types::Type;

/// A violation found during constructor-stub checking.
#[derive(Debug)]
enum Violation {
    /// An ADT's encoded type is a raw TyVar (stale stub encoding)
    StaleEncoding { type_name: String, tyvar: String },
    /// A constructor field type is a raw TyVar matching an ADT name
    StaleFieldType {
        type_name: String,
        constructor: String,
        field_index: usize,
        tyvar: String,
    },
}

/// Context for recursive stale-TyVar detection in constructor field types.
struct StubCheckCtx<'a> {
    adt_names: &'a BTreeSet<&'a str>,
    mu_bound: &'a BTreeSet<String>,
    parent_type: &'a str,
    ctor_name: &'a str,
    field_index: usize,
}

/// Entry point for `tungsten doctor check type integrity constructor-stubs <file>`.
pub fn cmd_check_constructor_stubs(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let adt_names: BTreeSet<&str> = project.adt_types.keys().map(|s| s.as_str()).collect();

    let violations = collect_violations(&project, &adt_names);
    report_violations(&project, &violations)
}

fn collect_violations(
    project: &driver::ProjectOutput,
    adt_names: &BTreeSet<&str>,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Check 1: encoded types should not be raw TyVars at top level
    for (name, encoded) in &project.encoded_types {
        if let Type::TyVar(tv) = encoded {
            if adt_names.contains(tv.as_str()) {
                violations.push(Violation::StaleEncoding {
                    type_name: name.clone(),
                    tyvar: tv.clone(),
                });
            }
        }
    }

    // Check 2: constructor field types should not be raw TyVars matching ADT names
    // (excluding μ-bound variables which are legitimate recursive references)
    let mut sorted_adts: Vec<&String> = project.adt_types.keys().collect();
    sorted_adts.sort();

    for adt_name in &sorted_adts {
        let (params, constructors) = &project.adt_types[*adt_name];
        let mut mu_bound = BTreeSet::new();
        mu_bound.insert((*adt_name).clone());
        for param in params {
            mu_bound.insert(param.clone());
        }
        collect_mu_binders_from_encoding(&project.encoded_types, adt_name, &mut mu_bound);
        for ctor in constructors {
            for (i, field_ty) in ctor.fields.iter().enumerate() {
                let ctx = StubCheckCtx {
                    adt_names,
                    mu_bound: &mu_bound,
                    parent_type: adt_name,
                    ctor_name: &ctor.name,
                    field_index: i,
                };
                check_type_for_stale_tyvar(field_ty, &ctx, &mut violations);
            }
        }
    }

    violations
}

fn report_violations(project: &driver::ProjectOutput, violations: &[Violation]) -> ExitCode {
    let total_adts = project.adt_types.len();
    let total_ctors: usize = project.adt_types.values().map(|(_, cs)| cs.len()).sum();

    if violations.is_empty() {
        println!("✓ {total_adts} ADTs, {total_ctors} constructors — no stale stubs");
        ExitCode::SUCCESS
    } else {
        println!(
            "✗ {total_adts} ADTs, {total_ctors} constructors — {} stale stub(s) found:\n",
            violations.len()
        );
        for v in violations {
            match v {
                Violation::StaleEncoding { type_name, tyvar } => {
                    println!(
                        "  {type_name}: encoded type is TyVar(\"{tyvar}\") — \
                         expected Sum/Product/Mu encoding"
                    );
                }
                Violation::StaleFieldType {
                    type_name,
                    constructor,
                    field_index,
                    tyvar,
                } => {
                    println!(
                        "  {type_name}::{constructor} field {field_index}: \
                         TyVar(\"{tyvar}\") — expected concrete type"
                    );
                }
            }
        }
        println!(
            "\nHint: stale constructor stubs indicate Signature Collection export \
             deduplication may have failed. Check extract_global_exports \
             and dedup_constructors in combined.tg."
        );
        ExitCode::FAILURE
    }
}

/// Recursively check a type tree for raw TyVars matching known ADT names.
///
/// Non-uniform arms: the guarded `TyVar` leaf and the `Mu` binder (which
/// extends `mu_bound` around the body walk). Every other variant delegates
/// to [`Type::children`] (ADR 23.7.26b) — including `Adt`, whose type args
/// and variant payloads the pre-sweep hand-rolled walker silently skipped
/// via its `_ => {}` fallthrough.
fn check_type_for_stale_tyvar(ty: &Type, ctx: &StubCheckCtx<'_>, violations: &mut Vec<Violation>) {
    match ty {
        Type::TyVar(tv) if ctx.adt_names.contains(tv.as_str()) && !ctx.mu_bound.contains(tv) => {
            violations.push(Violation::StaleFieldType {
                type_name: ctx.parent_type.to_string(),
                constructor: ctx.ctor_name.to_string(),
                field_index: ctx.field_index,
                tyvar: tv.clone(),
            });
        }
        Type::Mu(binder, body) => {
            let mut inner_bound = ctx.mu_bound.clone();
            inner_bound.insert(binder.clone());
            let inner_ctx = StubCheckCtx {
                mu_bound: &inner_bound,
                ..*ctx
            };
            check_type_for_stale_tyvar(body, &inner_ctx, violations);
        }
        _ => {
            for child in ty.children() {
                check_type_for_stale_tyvar(child, ctx, violations);
            }
        }
    }
}

/// Collect all μ-binder names from an ADT's type encoding.
/// For recursive types like `Mu(List, ...)`, adds "List" to `out`.
/// For mutually recursive types, follows nested Mu binders.
fn collect_mu_binders_from_encoding(
    encoded_types: &std::collections::HashMap<String, Type>,
    adt_name: &str,
    out: &mut BTreeSet<String>,
) {
    if let Some(encoded) = encoded_types.get(adt_name) {
        collect_mu_binders(encoded, out);
    }
}

/// Deliberately NOT on the `Type::children` discipline (ADR 23.7.26b): this
/// walks only the outer μ-spine (stopping at the first non-`Mu`), so
/// `children()` would over-traverse into subtrees it must never visit.
fn collect_mu_binders(ty: &Type, out: &mut BTreeSet<String>) {
    if let Type::Mu(binder, body) = ty {
        out.insert(binder.clone());
        collect_mu_binders(body, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stale_tyvars_in(ty: &Type, adt_names: &[&str], mu_bound: &[&str]) -> Vec<String> {
        let adt_names: BTreeSet<&str> = adt_names.iter().copied().collect();
        let mu_bound: BTreeSet<String> = mu_bound.iter().map(|s| s.to_string()).collect();
        let ctx = StubCheckCtx {
            adt_names: &adt_names,
            mu_bound: &mu_bound,
            parent_type: "Parent",
            ctor_name: "MkParent",
            field_index: 0,
        };
        let mut violations = Vec::new();
        check_type_for_stale_tyvar(ty, &ctx, &mut violations);
        violations
            .into_iter()
            .map(|v| match v {
                Violation::StaleFieldType { tyvar, .. }
                | Violation::StaleEncoding { tyvar, .. } => tyvar,
            })
            .collect()
    }

    #[test]
    fn stale_tyvar_leaf_is_flagged() {
        let ty = Type::TyVar("List".into());
        assert_eq!(stale_tyvars_in(&ty, &["List"], &[]), vec!["List"]);
    }

    #[test]
    fn mu_bound_tyvar_is_not_flagged() {
        // Mu(List, TyVar(List)) — the binder legitimises the reference
        let ty = Type::mu("List", Type::TyVar("List".into()));
        assert!(stale_tyvars_in(&ty, &["List"], &[]).is_empty());
    }

    #[test]
    fn non_adt_tyvar_is_not_flagged() {
        let ty = Type::TyVar("T".into());
        assert!(stale_tyvars_in(&ty, &["List"], &[]).is_empty());
    }

    #[test]
    fn stale_tyvar_inside_adt_variant_is_flagged() {
        // The pre-ADR-23.7.26b hand-rolled walker silently skipped Adt
        // subtrees (`_ => {}`); the children() default now covers them.
        let ty = Type::adt(
            "Wrapper",
            vec![Type::TyVar("List".into())],
            vec![("V".into(), Type::TyVar("Tree".into()))],
        );
        assert_eq!(
            stale_tyvars_in(&ty, &["List", "Tree"], &[]),
            vec!["List", "Tree"]
        );
    }

    #[test]
    fn stale_tyvar_in_arrow_children_is_flagged() {
        let ty = Type::arrow(Type::TyVar("List".into()), Type::Nat);
        assert_eq!(stale_tyvars_in(&ty, &["List"], &[]), vec!["List"]);
    }
}
