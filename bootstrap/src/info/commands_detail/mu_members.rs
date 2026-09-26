//! `tungsten info type mu-members` — what each μ-binder in a type's chain
//! actually denotes (ADR 1.8.26b close-out).
//!
//! ## The question this exists to answer
//!
//! A mutually recursive cluster encodes as **one nested μ-binder per SCC
//! member**, all wrapping the entry member's body:
//!
//! ```text
//! Alpha = μα_Alpha. μα_Beta. μα_Gamma. (Unit + (α_Beta × α_Alpha))
//! ```
//!
//! Read that as a closed type and `α_Beta` denotes `μα_Beta. μα_Gamma. (Unit +
//! (α_Beta × α_Alpha))` — i.e. "Beta = Unit + (Beta × Alpha)", which is **not
//! what Beta is**. The encoding does not carry the other members' bodies;
//! `α_Beta` is a marker saying "Beta goes here", and only the elaborator's
//! μ-provenance table knows what Beta is.
//!
//! That is a genuinely surprising property, and reading the encoding without it
//! yields confident wrong answers — ADR 1.8.26b's D2 root-cause thesis was
//! written that way and had to be retracted after measurement. This command
//! prints the resolution so the question takes one command instead of an
//! instrumented build.
//!
//! Cost 3 (elaboration only; the walk is a lookup over data the elaborator
//! already holds).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::comparator::ComparatorTypes;
use tungsten_bootstrap::driver::ProjectOutput;
use tungsten_core::types::Type;

use crate::info::elaborate_for_info;

use super::diagnostic::print_type_not_found;

/// One binder in a type's μ-chain, and what it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinderResolution {
    /// The binder as it appears in the encoding (`α_Beta`).
    pub binder: String,
    /// The ADT μ-provenance says it came from, if recorded.
    pub origin: Option<String>,
    /// How synthesis resolves it.
    pub verdict: BinderVerdict,
}

/// What a binder resolves to, from the *synthesis* point of view — the same
/// lookup `mu_comparator` performs, so this cannot disagree with a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BinderVerdict {
    /// The outermost binder is the type itself; nothing to look up.
    SelfReference,
    /// Provenance names a member with its own stored encoding.
    Resolvable { encoding: String },
    /// Provenance names a member, but it has no standalone stored encoding —
    /// the generic-ADT case (`List<T>`), where the binder is the enclosing type
    /// and needs no lookup.
    Generic { adt: String },
    /// Nothing resolves this binder, so a comparator over this type cannot be
    /// synthesized past it.
    Unresolvable,
}

/// Display a type's μ-binder chain and what each binder denotes.
pub fn cmd_info_mu_members(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };
    let Some(encoded) = project.encoded_types.get(name) else {
        print_type_not_found(name, &project);
        return ExitCode::FAILURE;
    };

    let types = ComparatorTypes::new(
        project.record_types.clone(),
        &project.encoded_types,
        &project.type_provenance,
        project.adt_types.clone(),
        &project.mutual_recursion_groups,
    );
    let chain = resolve_chain(encoded, &project, &types);
    print!("{}", render_report(name, &chain));
    ExitCode::SUCCESS
}

/// Resolve every binder in the type's outer μ-chain.
///
/// Only the *outer* chain: that is the one `mu_comparator` peels, and the one
/// whose members must resolve for a comparator to exist. Binders nested inside
/// a component belong to that component's own chain and are reported when you
/// ask about it.
pub(crate) fn resolve_chain(
    ty: &Type,
    project: &ProjectOutput,
    types: &ComparatorTypes,
) -> Vec<BinderResolution> {
    let mut out = Vec::new();
    let mut current = ty;
    while let Type::Mu(binder, body) = current {
        let origin = project
            .type_provenance
            .mu_origins
            .get(binder)
            .map(|o| o.adt_name.clone());
        let verdict = if out.is_empty() {
            BinderVerdict::SelfReference
        } else {
            classify_binder(binder, origin.as_deref(), types)
        };
        out.push(BinderResolution {
            binder: binder.clone(),
            origin,
            verdict,
        });
        current = body;
    }
    out
}

/// How a non-outermost binder resolves.
///
/// Delegates the lookup to [`ComparatorTypes::mu_member`] — the same call
/// `mu_comparator` makes — so a binder this reports `Resolvable` is one
/// synthesis can actually use.
fn classify_binder(binder: &str, origin: Option<&str>, types: &ComparatorTypes) -> BinderVerdict {
    match (types.mu_member(binder), origin) {
        (Some(encoding), _) => BinderVerdict::Resolvable {
            encoding: encoding.to_string(),
        },
        // Provenance names an ADT with no standalone encoding: a generic one.
        (None, Some(adt)) => BinderVerdict::Generic {
            adt: adt.to_string(),
        },
        (None, None) => BinderVerdict::Unresolvable,
    }
}

/// Render the chain, and say what it means for comparison.
pub(crate) fn render_report(name: &str, chain: &[BinderResolution]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "μ-binders: {name}");
    let _ = writeln!(out, "{}", "\u{2550}".repeat(11 + name.len()));
    let _ = writeln!(out);

    if chain.is_empty() {
        let _ = writeln!(
            out,
            "`{name}` is not recursive — its stored encoding has no μ-binder."
        );
        return out;
    }

    let _ = writeln!(out, "Binder chain (outermost first):");
    let width = chain.iter().map(|b| b.binder.len()).max().unwrap_or(0);
    for entry in chain {
        let _ = writeln!(
            out,
            "  {:<width$}  {}",
            entry.binder,
            describe(entry),
            width = width
        );
    }
    let _ = writeln!(out);

    if chain.len() == 1 {
        let _ = writeln!(
            out,
            "One binder: ordinary self-recursion. Nothing to resolve."
        );
        return out;
    }

    let _ = writeln!(
        out,
        "{} binders means a mutually recursive cluster. **The encoding does not",
        chain.len()
    );
    let _ = writeln!(
        out,
        "carry the other members' bodies** — read `{name}` as a closed type and",
    );
    let _ = writeln!(
        out,
        "an inner binder appears to denote the ENTRY member's body, which is not"
    );
    let _ = writeln!(
        out,
        "what that member is. Only μ-provenance knows; that is what the right-"
    );
    let _ = writeln!(out, "hand column above resolves (ADR 1.8.26b D2).");

    let unresolved: Vec<&BinderResolution> = chain
        .iter()
        .filter(|b| {
            !matches!(
                b.verdict,
                BinderVerdict::Resolvable { .. } | BinderVerdict::SelfReference
            )
        })
        .collect();
    if !unresolved.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{} binder(s) do not resolve to a stored member encoding, so a",
            unresolved.len()
        );
        let _ = writeln!(
            out,
            "comparator over `{name}` cannot be synthesized past them:"
        );
        for entry in unresolved {
            let _ = writeln!(out, "    {}", entry.binder);
        }
        let _ = writeln!(
            out,
            "  (`tungsten doctor check comparable {name} <file>` reports the same"
        );
        let _ = writeln!(out, "   thing as the gate's own verdict.)");
    }
    out
}

/// The right-hand column: what this binder denotes, in one line.
fn describe(entry: &BinderResolution) -> String {
    let origin = entry.origin.as_deref().unwrap_or("<no provenance>");
    match &entry.verdict {
        BinderVerdict::SelfReference => format!("→ {origin}  (this type)"),
        BinderVerdict::Resolvable { encoding } => format!("→ {origin}  {encoding}"),
        BinderVerdict::Generic { adt } => {
            format!("→ {adt}  (generic — no standalone stored encoding)")
        }
        BinderVerdict::Unresolvable => "→ UNRESOLVED  (no μ-provenance entry)".to_string(),
    }
}

#[cfg(test)]
#[path = "mu_members_tests.rs"]
mod tests;
