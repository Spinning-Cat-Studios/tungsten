//! `info constructors` — inspect constructor list entries for a given ADT (ADR 7.5.26e).
//!
//! ## Why `--raw` exists (ADR 1.8.26c retrospective)
//!
//! The default rendering formats each field through `Display`, which **strips
//! the Type-Body Collection `@`-prefix**: `TyVar("List")` and `TyVar("@List")`
//! both print as `List`. That prefix is not decoration — `records()` and
//! `encoded_types` are keyed without it, so an occurrence that survives into a
//! walk surfaces as `@Ident is not defined` about a record the project defines.
//! ADR 1.8.26c measured exactly that: 81 incomplete closures, one cause.
//!
//! `Display` *does* separate a zero-argument application (`List<>`) from a bare
//! `TyVar` (`List`) — the 1.8.26c retrospective claimed otherwise and the test
//! below pins the correction. What remains invisible without `--raw` is the
//! prefix, and with it any structure below the head.
//!
//! ADR 1.8.26c settled the question with a throwaway `cargo` example that
//! dumped `project.adt_types`. `--raw` is that example, productized.

// `writeln!` into a String: clippy's `format_push_string` lint rejects
// `push_str(&format!(..))`, and the fmt::Write impl for String is infallible —
// hence the discarded results below.
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use tungsten_bootstrap::elaborate::Constructor;

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
    raw: bool,
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

    if raw {
        println!();
        print!("{}", render_raw_fields(constructors));
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

/// The `--raw` section: every constructor's stored field types, verbatim.
///
/// Sorted by constructor index so two runs over the same ADT produce identical
/// output — `adt_types` is a `HashMap`, and a listing whose order changes
/// between invocations cannot be diffed.
///
/// Leads with a legend, because the spelling labels are this command's own
/// vocabulary: a reader meeting `app/0` for the first time should not have to
/// find this source file to learn it names a zero-argument application.
///
/// Returns the text rather than printing it, so the format is assertable
/// without capturing stdout.
#[must_use]
pub(crate) fn render_raw_fields(constructors: &[Constructor]) -> String {
    let mut ordered: Vec<&Constructor> = constructors.iter().collect();
    ordered.sort_by_key(|c| (c.index, c.name.clone()));

    let mut out = String::from("Raw field types (stored `Type`, pre-Display):\n");
    out.push_str(
        "  legend: tyvar=bare name, at-tyvar=@-prefixed (Type-Body Collection),\n\
         \x20         mu-binder=bound by an enclosing Mu, app/0=zero-argument\n\
         \x20         application, app/n=applied generic\n",
    );
    for ctor in ordered {
        if ctor.fields.is_empty() {
            let _ = writeln!(out, "  [{}] {} — no fields", ctor.index, ctor.name);
            continue;
        }
        for (position, field) in ctor.fields.iter().enumerate() {
            let _ = writeln!(
                out,
                "  [{}] {}.{} {} := {:?}",
                ctor.index,
                ctor.name,
                position,
                render_spelling(field),
                field
            );
        }
    }
    out
}

/// A one-word name for the *spelling* of a field's head.
///
/// `at-tyvar` and `mu-binder` are the two `Display` renders indistinguishably
/// from a plain `tyvar` (it strips `@`, and `α_List` reads as a type name), and
/// both mean something quite different: a still-deferred reference, and a
/// variable bound by an enclosing `Mu`.
fn render_spelling(ty: &Type) -> &'static str {
    match ty {
        Type::TyVar(name) if name.starts_with('@') => "at-tyvar",
        Type::TyVar(name) if name.starts_with("α_") => "mu-binder",
        Type::TyVar(_) => "tyvar",
        Type::App(_, args) if args.is_empty() => "app/0",
        Type::App(_, _) => "app/n",
        Type::Adt(_, _, _) => "adt",
        Type::Mu(_, _) => "mu",
        _ => "structural",
    }
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

#[cfg(test)]
// Tests: constructors_tests.rs
#[path = "constructors_tests.rs"]
mod tests;
