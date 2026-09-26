//! Self-host error code catalogue for `tungsten explain error --self-hosted`.
//!
//! Maps self-hosted-compiler error codes to descriptions.
//! The self-hosted compiler uses range-based numbering that differs from the
//! bootstrap:
//!   self-host E0001 = `TypeMismatch`    vs  bootstrap E0010 = `TypeMismatch`
//!   self-host E0101 = `UnresolvedValue` vs  bootstrap E0001 = `UndefinedVariable`
//!
//! See `src/compiler/elab/error/kinds.tg` for the canonical source.

use std::fmt::Write as _;
use std::process::ExitCode;

mod entries;
// Test-only: the range table is a *description* of the catalogue that nothing
// in production reads, because every consumer looks a code up rather than
// classifying it. That is precisely why the blocks need asserting.
#[cfg(test)]
mod ranges;

use entries::SELF_HOSTED_ERRORS;

/// Display order for the listing. A category with no entries is skipped, so a
/// name here that no entry uses simply contributes nothing.
const CATEGORY_ORDER: &[&str] = &[
    "Type Errors",
    "Name Resolution",
    "Items",
    "Patterns",
    "Proofs",
    "References",
    "Control Flow",
    "Entry Point",
    "Soundness",
    "Limits",
    "Phases",
    "Internal",
];

/// The category-grouped body of the error listing (no header or footer).
fn render_error_list_body() -> String {
    let mut out = String::new();
    for cat in CATEGORY_ORDER {
        let entries: Vec<_> = SELF_HOSTED_ERRORS
            .iter()
            .filter(|e| e.category == *cat)
            .collect();
        if entries.is_empty() {
            continue;
        }
        // Writing into a String is infallible, so each `write!` result is dropped.
        let _ = writeln!(out, "{cat}:");
        for entry in entries {
            let _ = writeln!(
                out,
                "  {} {:<30} {}",
                entry.code,
                entry.name,
                short_desc(entry.description)
            );
        }
        out.push('\n');
    }
    out
}

/// The full self-host error listing — header, category-grouped body, footer.
///
/// Returned as a `String` rather than printed so the whole thing is assertable:
/// the grouping, the omission of empty categories, and the presence of every
/// catalogue entry. A `print_*` function would leave all of that invisible to
/// tests, since nothing downstream inspects stdout.
pub fn self_hosted_error_list() -> String {
    let mut out = String::from(
        "Self-Hosted Compiler Error Reference\n\
         ══════════════════════════════════════════\n\
         \n\
         Note: self-host error codes differ from bootstrap (Rust) codes.\n\
         The bootstrap uses flat numbering; the self-hosted compiler uses ranges.\n\
         See `tungsten explain error` for bootstrap codes.\n\n",
    );
    out.push_str(&render_error_list_body());
    out.push_str(
        "Use `tungsten explain error --self-hosted <code>` for detailed explanation.\n\
         Use `tungsten explain error --self-hosted <name>` to look up by name.\n",
    );
    out
}

/// The catalogue entry `query` names, by code or by kind name.
///
/// A named function rather than an inline `find`, so what `explain error
/// --self-hosted <code>` resolves is assertable: the printer returns an
/// `ExitCode`, which implements no equality, and only spawning the binary
/// would otherwise pin which queries succeed.
fn lookup(query: &str) -> Option<&'static entries::SelfHostedErrorEntry> {
    SELF_HOSTED_ERRORS.iter().find(|entry| {
        entry.code.eq_ignore_ascii_case(query) || entry.name.eq_ignore_ascii_case(query)
    })
}

/// Print a detailed self-host error explanation.
pub fn print_self_hosted_error_explanation(query: &str) -> ExitCode {
    if let Some(entry) = lookup(query) {
        println!("Self-host error: {} ({})", entry.code, entry.name);
        println!("{}", "═".repeat(12 + entry.code.len() + entry.name.len()));
        println!();
        println!("Category: {}", entry.category);
        println!();
        println!("Description:");
        for line in entry.description.lines() {
            println!("  {line}");
        }
        println!();
        // Show the bootstrap equivalent if known
        if let Some(bootstrap) = bootstrap_equivalent(entry.code) {
            println!("bootstrap equivalent: tungsten explain error {bootstrap}");
        }
        println!();
        println!("Note: self-host codes appear in tungsten1/tungsten2 output.");
        println!("Bootstrap codes appear in the Rust bootstrap compiler output.");
        ExitCode::SUCCESS
    } else {
        eprintln!("Unknown self-host error: `{query}`");
        if let Some(suggestion) = fuzzy_match_self_hosted(query) {
            eprintln!("Did you mean `{suggestion}`?");
        }
        eprintln!();
        eprintln!("Run `tungsten explain error --self-hosted` to list all self-host error codes.");
        ExitCode::FAILURE
    }
}

/// Map a self-host code → bootstrap name for cross-reference.
fn bootstrap_equivalent(self_hosted_code: &str) -> Option<&'static str> {
    match self_hosted_code {
        "E0001" => Some("TypeMismatch"),
        "E0002" => Some("ArityMismatch"),
        "E0003" => Some("ExpectedFunction"),
        "E0004" => Some("UndefinedType"),
        "E0005" => Some("ExpectedType"),
        "E0008" => Some("CannotInferType"),
        "E0100" => Some("UndefinedType"),
        "E0101" => Some("UndefinedVariable"),
        "E0102" => Some("ModuleNotFound"),
        "E0103" => Some("DuplicateDefinition"),
        "E0104" => Some("PrivateItem"),
        "E0106" => Some("DuplicateImport"),
        "E0300" => Some("NonExhaustiveMatch"),
        "E0301" => Some("UnreachableArm"),
        "E0307" => Some("PatternTooDeep"),
        "E0601" => Some("NoMainFunction"),
        _ => None,
    }
}

/// Truncate a description for listing display.
fn short_desc(desc: &str) -> &str {
    let end = desc.find('.').map_or(desc.len(), |i| i);
    &desc[..end]
}

/// Fuzzy-match a self-host error code or name.
fn fuzzy_match_self_hosted(input: &str) -> Option<&'static str> {
    let input_lower = input.to_lowercase();

    // Try prefix match on codes
    if input_lower.starts_with('e') {
        for entry in SELF_HOSTED_ERRORS {
            if entry.code.to_lowercase().starts_with(&input_lower) {
                return Some(entry.code);
            }
        }
    }

    // Try substring match on names
    for entry in SELF_HOSTED_ERRORS {
        if entry.name.to_lowercase().contains(&input_lower) {
            return Some(entry.name);
        }
    }

    None
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
