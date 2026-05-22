//! Static error catalogue for `tungsten explain error`.
//!
//! Every `ElabErrorKind` variant has a corresponding explanation entry,
//! enforced by an exhaustive match in `get_explanation()`.
//!
//! The explanation data lives in `explanations.rs` (per-category functions).

use std::process::ExitCode;

/// A static explanation for one `ElabErrorKind` variant.
pub(super) struct ErrorExplanation {
    pub name: &'static str,
    pub category: &'static str,
    #[allow(dead_code)]
    pub summary: &'static str,
    pub detail: &'static str,
    pub example: &'static str,
    pub see_also: &'static [&'static str],
}

/// All error kinds grouped by category, for listing.
struct ErrorCategory {
    name: &'static str,
    entries: &'static [(&'static str, &'static str)], // (kind_name, summary)
}

const CATEGORIES: &[ErrorCategory] = &[
    ErrorCategory {
        name: "Name Resolution",
        entries: &[
            ("UndefinedVariable", "cannot find value in scope"),
            ("UndefinedType", "cannot find type in scope"),
            ("UndefinedConstructor", "cannot find constructor in scope"),
            ("DuplicateDefinition", "name defined multiple times"),
            ("ModuleNotFound", "cannot find referenced module"),
            ("ItemNotFoundInModule", "item not found in module"),
            ("DuplicateImport", "same name imported twice"),
            ("GlobConflict", "glob imports conflict on a name"),
            ("UnresolvedImport", "cannot resolve import path"),
            ("PrivateModule", "module is private"),
            ("PrivateItem", "item is private"),
            ("PublicItemLeak", "public item exposes private type"),
        ],
    },
    ErrorCategory {
        name: "Type Errors",
        entries: &[
            ("TypeMismatch", "expected one type, found another"),
            ("CannotInferType", "type annotation needed"),
            ("CannotInferTypeArg", "cannot infer type argument"),
            ("ArityMismatch", "wrong number of arguments"),
            ("ExpectedFunction", "expected function, found other type"),
            ("ExpectedType", "expected a specific type"),
        ],
    },
    ErrorCategory {
        name: "Phase 1 Restrictions",
        entries: &[
            ("UnsupportedFeature", "feature not yet supported"),
            ("MutabilityNotSupported", "mutable bindings not supported"),
        ],
    },
    ErrorCategory {
        name: "Pattern Matching",
        entries: &[
            ("NonExhaustiveMatch", "match does not cover all cases"),
            ("UnreachableArm", "pattern is unreachable"),
            ("PatternTooDeep", "pattern nesting exceeds limit"),
            ("UnsupportedPattern", "pattern form not supported"),
        ],
    },
    ErrorCategory {
        name: "Entry Point",
        entries: &[
            ("NoMainFunction", "no main function found"),
            ("ContainsSorry", "file contains sorry (cannot compile)"),
        ],
    },
    ErrorCategory {
        name: "Control Flow",
        entries: &[
            ("DeadCodeAfterReturn", "unreachable code after return"),
            ("TryOnNonTryType", "? on non-Result/Option type"),
            ("TryReturnMismatch", "? return type mismatch"),
            ("TryOutsideReturnContext", "? outside function body"),
            ("LetElseNonDiverging", "let-else branch does not diverge"),
            ("LetElseIrrefutable", "irrefutable pattern in let-else"),
            ("IfLetIrrefutable", "irrefutable pattern in if let"),
        ],
    },
    ErrorCategory {
        name: "Named Records",
        entries: &[
            ("NotARecordType", "type is not a record"),
            ("MissingRecordField", "missing field in record constructor"),
            ("ExtraRecordField", "unknown field in record constructor"),
            ("DuplicateRecordField", "field specified twice"),
        ],
    },
];

/// Print the grouped error listing.
pub(super) fn print_error_list() {
    println!("Tungsten Error Reference");
    println!("════════════════════════");
    println!();

    for cat in CATEGORIES {
        println!("{}:", cat.name);
        for (name, summary) in cat.entries {
            println!("  {name:<26} {summary}");
        }
        println!();
    }

    println!("Use `tungsten explain error <name>` for detailed explanation.");
}

/// Print a detailed explanation for a specific error kind.
pub(super) fn print_error_explanation(name: &str) -> ExitCode {
    if let Some(exp) = get_explanation(name) {
        println!("Error: {}", exp.name);
        println!("{}", "═".repeat(8 + exp.name.len()));
        println!();
        println!("Category: {}", exp.category);
        println!();
        println!("What it means:");
        for line in exp.detail.lines() {
            println!("  {line}");
        }
        println!();
        println!("Example:");
        for line in exp.example.lines() {
            println!("  {line}");
        }
        if !exp.see_also.is_empty() {
            println!();
            println!("See also:");
            for related in exp.see_also {
                println!("  • tungsten explain error {related}");
            }
        }
        ExitCode::SUCCESS
    } else {
        eprintln!("Unknown error kind: `{name}`");
        // Fuzzy suggest
        if let Some(suggestion) = fuzzy_match(name) {
            eprintln!("Did you mean `{suggestion}`?");
        }
        eprintln!();
        eprintln!("Run `tungsten explain error` to list all error kinds.");
        ExitCode::FAILURE
    }
}

/// Fuzzy-match an error kind name using Levenshtein distance.
fn fuzzy_match(input: &str) -> Option<&'static str> {
    let input_lower = input.to_lowercase();
    let mut best: Option<(&'static str, usize)> = None;

    for cat in CATEGORIES {
        for (name, _) in cat.entries {
            let name_lower = name.to_lowercase();
            let dist = levenshtein(&input_lower, &name_lower);
            if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
                best = Some((name, dist));
            }
        }
    }

    best.map(|(name, _)| name)
}

/// Simple Levenshtein distance (sufficient for ~25 error kinds).
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ─────────────────────────────────────────────────────────────────────────────
// Exhaustive error catalogue
// ─────────────────────────────────────────────────────────────────────────────
//
// This uses a match on string names rather than importing ElabErrorKind directly,
// because the explain module lives in the binary crate (main.rs) not the library.
// Completeness is verified by a test that checks every ElabErrorKind variant name
// against this catalogue.
//
// The actual explanation data lives in `explanations.rs`, split by category.

fn get_explanation(name: &str) -> Option<ErrorExplanation> {
    super::explanations::get_explanation(name)
}

/// Return the list of all known error kind names (for testing completeness).
#[cfg(test)]
pub(super) fn all_known_names() -> Vec<&'static str> {
    CATEGORIES
        .iter()
        .flat_map(|cat| cat.entries.iter().map(|(name, _)| *name))
        .collect()
}

#[cfg(test)]
pub(super) fn get_explanation_by_name(name: &str) -> bool {
    get_explanation(name).is_some()
}
