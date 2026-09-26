//! Static error catalogue for `tungsten explain error`.
//!
//! Accepts an error **code** (`E0061`) or a kind **name**
//! (`NonStrictlyPositive`), case-insensitively — the code is what compiler
//! output prints and therefore what a user arrives holding (ADR 8.8.26a).
//!
//! Coverage is enforced by `every_error_code_resolves` in `tests.rs`, which
//! reads the real `ElabErrorKind::code()` arms. It replaced a hand-maintained
//! list that claimed exhaustiveness while 15 of 53 kinds had no entry.
//!
//! The explanation data lives in `explanations/` (per-category functions), and
//! the grouped `(kind, summary)` table in [`super::error_categories`].

use std::process::ExitCode;

use super::error_categories::CATEGORIES;

/// A static explanation for one `ElabErrorKind` variant.
pub(super) struct ErrorExplanation {
    pub name: &'static str,
    /// The code this kind renders as (`ElabErrorKind::code()`).
    ///
    /// Load-bearing, not decoration: the code is the **only** identifier that
    /// appears in compiler output, so it is what a user arrives holding
    /// (ADR 8.8.26a). `resolve_query` matches on it.
    pub code: &'static str,
    pub category: &'static str,
    #[allow(dead_code)]
    pub summary: &'static str,
    pub detail: &'static str,
    pub example: &'static str,
    pub see_also: &'static [&'static str],
}

/// The grouped error listing, as a value.
///
/// Returned rather than printed so the codes can be asserted: the golden
/// harness only invokes the compiler as `<cmd> <file.tg>`, so it cannot
/// exercise a subcommand that takes no file (ADR 8.8.26a §5, amended AC).
/// The caller prints it — a `print_error_list` wrapper would be an untestable
/// one-line shim, and was deleted for exactly that reason.
pub(super) fn render_error_list() -> String {
    use std::fmt::Write as _;

    let mut out = String::from("Tungsten Error Reference\n════════════════════════\n\n");
    for cat in CATEGORIES {
        let _ = writeln!(out, "{}:", cat.name);
        for (name, summary) in cat.entries {
            // The code first: it is what the reader arrived with.
            let code = get_explanation(name).map_or("     ", |e| e.code);
            let _ = writeln!(out, "  {code:<7} {name:<26} {summary}");
        }
        out.push('\n');
    }
    out.push_str("Use `tungsten explain error <code|name>` for detailed explanation.\n");
    out
}

/// Print a detailed explanation for a specific error kind.
pub(super) fn print_error_explanation(query: &str) -> ExitCode {
    let name = resolve_query(query).unwrap_or(query);
    if let Some(exp) = get_explanation(name) {
        println!("Error: {} ({})", exp.name, exp.code);
        println!("{}", "═".repeat(11 + exp.name.len() + exp.code.len()));
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
    } else if is_code_shaped(query) {
        // Reaching here means the code did not resolve, so do not suggest a
        // near-miss NAME — the two failures want different advice.
        eprintln!("Unknown error code: `{query}`");
        eprintln!();
        eprint!("{UNKNOWN_CODE_ADVICE}");
        ExitCode::FAILURE
    } else {
        eprintln!("Unknown error kind: `{query}`");
        // Fuzzy suggest
        if let Some(suggestion) = fuzzy_match(query) {
            eprintln!("Did you mean `{suggestion}`?");
        }
        eprintln!();
        eprint!("{UNKNOWN_NAME_ADVICE}");
        ExitCode::FAILURE
    }
}

/// What to try after a code-shaped query fails to resolve.
///
/// A `const` rather than inline `eprintln!`s for the same reason
/// [`render_error_list`] returns its string: so the claim it makes about the
/// listing is a value a test can read. Nothing had ever read this claim, and it
/// spent four days saying the listing held every code (ADR 19.8.26b).
pub(super) const UNKNOWN_CODE_ADVICE: &str = "\
Codes are accepted, so this one is not in the catalogue.
Run `tungsten explain error` to list every user-facing code.
For a self-hosted-compiler code, add --self-hosted.
";

/// What to try after a kind NAME fails to resolve.
///
/// Distinct from [`UNKNOWN_CODE_ADVICE`] because the two failures want
/// different advice: a misspelt name gets a fuzzy suggestion and a pointer at
/// codes, a bad code gets neither.
pub(super) const UNKNOWN_NAME_ADVICE: &str = "\
Run `tungsten explain error` to list every user-facing kind.
An error code (e.g. `E0010`) is accepted here too.
";

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
// Dispatch is keyed on the kind NAME; `resolve_query` maps a code onto a name
// first, so there is exactly one lookup path. Completeness is verified by
// `every_error_code_resolves`, which reads the real `ElabErrorKind::code()`
// arms rather than a list typed beside it — the hand-maintained list it
// replaced could not detect a variant missing from itself, and 15 were
// (ADR 8.8.26a).
//
// The actual explanation data lives in `explanations.rs`, split by category.

fn get_explanation(name: &str) -> Option<ErrorExplanation> {
    super::explanations::get_explanation(name)
}

/// Map a user's query — an error **code** or a kind **name** — onto a kind name.
///
/// Codes are what compiler output prints (`error[E0061]`); kind names appear
/// nowhere a user can see them. Both are accepted, case-insensitively, exactly
/// as the self-hosted arm has always done
/// (`self_hosted_error_catalogue::print_self_hosted_error_explanation`).
pub(super) fn resolve_query(query: &str) -> Option<&'static str> {
    for cat in CATEGORIES {
        for (name, _) in cat.entries {
            if name.eq_ignore_ascii_case(query) {
                return Some(name);
            }
            if get_explanation(name).is_some_and(|e| e.code.eq_ignore_ascii_case(query)) {
                return Some(name);
            }
        }
    }
    // Resolvable but deliberately unlisted: a user holding an internal error
    // code can look it up, but the no-argument listing does not advertise a
    // code no program edit can cause or fix (ADR 15.8.26b).
    UNLISTED_KINDS
        .iter()
        .find(|name| {
            name.eq_ignore_ascii_case(query)
                || get_explanation(name).is_some_and(|e| e.code.eq_ignore_ascii_case(query))
        })
        .copied()
}

/// Kinds that resolve in `explain error <code|name>` but are excluded from
/// the no-argument listing — see [`resolve_query`].
const UNLISTED_KINDS: &[&str] = &["InternalError"];

/// Whether `query` looks like an error code rather than a kind name.
///
/// Used only on the failure path, to tell "you spelled the code wrong" apart
/// from "you spelled the name wrong" — the two want different advice.
pub(super) fn is_code_shaped(query: &str) -> bool {
    let mut chars = query.chars();
    matches!(chars.next(), Some('E' | 'e' | 'W' | 'w'))
        && query.len() == 5
        && chars.all(|c| c.is_ascii_digit())
}

/// Test-visible [`UNLISTED_KINDS`], so the prose check reads the constant the
/// CLI resolves against rather than a copy maintained beside the test — a check
/// keyed on its own copy is the defect ADR 19.8.26b exists to fix.
#[cfg(test)]
pub(super) fn unlisted_kinds() -> &'static [&'static str] {
    UNLISTED_KINDS
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

/// Test-visible [`resolve_query`], so the coverage oracle drives the same
/// lookup the CLI does rather than a re-implementation of it.
#[cfg(test)]
pub(super) fn resolve_query_for_test(query: &str) -> Option<&'static str> {
    resolve_query(query)
}

/// Test-visible [`is_code_shaped`].
#[cfg(test)]
pub(super) fn is_code_shaped_for_test(query: &str) -> bool {
    is_code_shaped(query)
}

/// `(detail, example, code)` for one entry, for the substance check.
#[cfg(test)]
pub(super) fn entry_body_for_test(
    name: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    get_explanation(name).map(|e| (e.detail, e.example, e.code))
}
