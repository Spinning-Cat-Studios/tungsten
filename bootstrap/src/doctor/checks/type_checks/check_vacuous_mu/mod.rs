//! `tungsten doctor check type vacuous-mu` — which types encode to `μX. X`?
//!
//! **The question this answers, and why it needs a command.** A *nested*
//! inductive family — one whose recursive occurrence sits under a generic
//! parameter, `type Rose = Node(Wrap<Rose>)` — has nowhere to put the
//! recursion, so its cached encoding collapses to a **vacuous** binder whose
//! body is just the binder. No unfold can reach a structural head from that, so
//! every `match` on such a type is rejected with E0064.
//!
//! The trap is *when* that happens. The type definition itself is accepted and
//! checks clean in microseconds; the rejection lands only once a term matches
//! on it. So a project can carry the defect invisibly — through review, through
//! CI, through every gate — until someone writes the first `match`. This check
//! is the thing that finds it first.
//!
//! **Why not `info type type-encoding`.** That command prints one named type's
//! chain, and — being an elaboration-driven tool — it is *blocked by E0064
//! itself* on any file that already matches on a nested family. It answers
//! "what is this type's encoding" for a file that still compiles. This answers
//! "does this project contain the shape" across every type at once, which is
//! the question worth asking before the first `match` exists.
//!
//! Cost 3: elaboration only, no codegen.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use crate::driver;

#[cfg(test)]
mod tests;

/// One type whose cached encoding cannot be unfolded to a structural head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacuousType {
    /// The type's source name.
    pub name: String,
    /// The binder whose body is the binder itself (`α_Rose`).
    pub binder: String,
}

/// Whether `type_name`'s encoding is the vacuous `μX. X` — a μ chain whose
/// body is a bare occurrence of the binder standing for **`type_name` itself**.
///
/// Returns that binder, which is the one a diagnostic should name.
///
/// # Why the type's own name is required, and not just the chain
///
/// "The body is one of the binders" is **not** the test, and getting this wrong
/// makes the check fire on healthy code. Under the ADR 18.4.26i group encoding
/// a mutually recursive cluster's binders all wrap the *entry* member's body,
/// so an inner `α_Beta` is a **marker** meaning "Beta goes here" — not a type,
/// and emphatically not a vacuous self-reference. The perfectly legal
///
/// ```text
/// type RoseKids = NoKids | Kid(Rose, RoseKids)
/// type Rose     = Node(RoseKids)
/// ```
///
/// encodes `Rose` as `μα_RoseKids. α_RoseKids`: a chain binder in body
/// position, and yet the pair compiles and matches fine. Reading that encoding
/// as closed is the "confident wrong answer" `.claude/CLAUDE.md` warns about,
/// and it is what the first draft of this check did.
///
/// Comparing against `type_name` separates the two: `α_Rose` in *`Rose`'s* own
/// encoding is genuine self-reference with no body, while `α_RoseKids` there
/// points at a sibling that has one. Conservative by construction — a guard
/// that fires when nothing is wrong is worse than no guard, because it teaches
/// its readers to ignore exit codes.
///
/// # Termination
///
/// A **descent**: it walks strictly into its input and never substitutes, so it
/// terminates on any type — including the ones that made
/// `unfold_inner_mu_layers` spin (ADR 11.8.26c). That is precisely why this
/// check can run over a corpus the peel cannot.
#[must_use]
pub fn vacuous_binder(type_name: &str, ty: &Type) -> Option<String> {
    let mut sawmu = false;
    let mut body = ty;
    while let Type::Mu(_, inner) = body {
        sawmu = true;
        body = inner;
    }
    if !sawmu {
        return None;
    }
    match body {
        Type::TyVar(binder) if binder.strip_prefix("α_").unwrap_or(binder) == type_name => {
            Some(binder.clone())
        }
        _ => None,
    }
}

/// Every `(name, binder)` in `encodings` whose encoding is vacuous, sorted by
/// name so the report is stable across runs.
///
/// Pure over an injected map rather than reaching for a `ProjectOutput`: it is
/// what makes the shape assertable without elaborating anything.
#[must_use]
pub fn vacuous_types<'a>(
    encodings: impl IntoIterator<Item = (&'a String, &'a Type)>,
) -> Vec<VacuousType> {
    let mut found: Vec<VacuousType> = encodings
        .into_iter()
        .filter_map(|(name, ty)| {
            vacuous_binder(name, ty).map(|binder| VacuousType {
                name: name.clone(),
                binder,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Render the census. Returns `(text, is_clean)`.
///
/// Separated from the command so both arms are assertable — an all-`SUCCESS`
/// exit-code test asserts nothing.
#[must_use]
pub fn render(found: &[VacuousType], total: usize, file: &str) -> (String, bool) {
    if found.is_empty() {
        // `0 examined` and `0 found` must not read alike — the reach-line rule.
        // Zero encodings is legitimate here (a file of only parameterized types
        // caches none), so it is a note rather than an error; but a bare "✓"
        // over nothing is how a check gets trusted for work it never did.
        if total == 0 {
            return (
                format!(
                    "✓ no vacuous μ-encodings in {file} — but NOTHING WAS EXAMINED: \
                     the file caches 0 type encodings.\n  \
                     Legitimate for a file whose types are all parameterized \
                     (those get no cached encoding), and otherwise a sign you \
                     pointed this at the wrong file.\n"
                ),
                true,
            );
        }
        return (
            format!("✓ no vacuous μ-encodings in {file} ({total} type encoding(s) checked)\n"),
            true,
        );
    }
    let mut out = format!(
        "✗ {} of {total} type encoding(s) in {file} collapsed to a vacuous μ:\n\n",
        found.len()
    );
    for VacuousType { name, binder } in found {
        out.push_str(&format!("  {name}  encodes to  μ{binder}. {binder}\n"));
    }
    out.push_str(
        "\n  These are nested inductive families: the recursive occurrence sits under a\n  \
         generic parameter, so the encoding has nowhere to put it. The definitions are\n  \
         accepted, but any `match` on one is rejected with E0064 — this check is what\n  \
         finds them BEFORE the first match is written.\n\n  \
         Fix by breaking the nesting with a non-generic intermediate type. See\n  \
         `tungsten explain error NestedRecursiveFamily`.\n",
    );
    (out, false)
}

/// Entry point for `tungsten doctor check type vacuous-mu <file>`.
pub fn cmd_check_vacuous_mu(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(project) => project,
        Err(why) => {
            eprintln!("error: {why}");
            return ExitCode::FAILURE;
        }
    };

    let found = vacuous_types(project.encoded_types.iter());
    let (report, clean) = render(
        &found,
        project.encoded_types.len(),
        &file.display().to_string(),
    );
    print!("{report}");

    if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
