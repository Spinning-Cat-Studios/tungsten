//! Layer 1 of the prose check: the judgement, as a pure function.
//!
//! Nothing here touches clap or the CLI — [`claim_is_consistent`] is exercised
//! over injected strings, which is what makes the four cases decidable one at a
//! time and the whole thing mutable and assertable. [`super::surfaces`] is what
//! binds it to what a user actually sees.

use crate::explain::error_catalogue;

/// How strictly one description has to account for a withheld entry.
///
/// Two levels rather than one because the strings are not one kind of text: a
/// paragraph of long help has room to name the exception, a one-line failure
/// hint does not. A single rule demanding the name everywhere would only be
/// satisfiable by growing an `E9998` clause inside `hint: …`, degrading the
/// output the check exists to protect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ClaimStrictness {
    /// One-line hints: the qualifier alone.
    Qualifies,
    /// Long help and triage copy: the qualifier **and** the named exception.
    Explains,
}

/// The adjective that admits an exception to an otherwise exhaustive claim.
const QUALIFIER: &str = "user-facing";

/// Words that turn a listing verb into a claim of exhaustiveness.
const TOTALITY_WORDS: &[&str] = &["every", "all"];

/// What such a claim is about, when it says.
const LISTING_NOUNS: &[&str] = &["code", "codes", "kind", "kinds"];

/// Verbs whose object may be elided — "Omit to list all." names no noun and is
/// a claim about the listing anyway. This is row 2 of ADR 19.8.26b §1.2, and a
/// noun-only scanner walks straight past it.
const LISTING_VERBS: &[&str] = &["list", "lists", "listing"];

/// How many words may sit between "every"/"all" and the noun it quantifies.
/// Three covers "every user-facing code" and "all error kinds".
const NOUN_WINDOW: usize = 3;

/// Phrases that mark something as held back from the listing.
///
/// Hand-maintained, and the one place in this file where that is a liability
/// rather than a definition: a description reworded to *"does not appear
/// below"* marks a real exclusion in words no entry here matches, and the
/// naming arm reddens correct prose. That is the safe direction to be wrong in
/// — but the message has to say so, or the next reader reads "names no
/// exception" and goes looking for a missing sentence that is right there. See
/// [`super::surfaces::no_description_of_the_listing_overpromises`]'s message.
const EXCLUSION_MARKERS: &[&str] = &[
    "absent from the listing",
    "does not appear in the listing",
    "excluded from the listing",
    "kept out of the listing",
    "not listed",
    "unlisted",
    "withheld",
];

/// Whether `text` describes the listing consistently with what it withholds.
///
/// `withheld` is the set of kinds the no-argument listing omits, as
/// `(kind name, code)` — a description may name either spelling.
///
/// It fails in **both** directions:
///
/// - With a non-empty set, an unqualified claim of exhaustiveness is false, and
///   at [`ClaimStrictness::Explains`] so is text that never names the
///   exception. The naming arm is what protects the explaining paragraph: a
///   check that looked only for the qualifier would watch that paragraph be
///   deleted without complaint.
/// - With an empty set, the naming requirement inverts — text still describing
///   an exception describes something that no longer exists. The *qualifier*
///   does not invert, because "every user-facing code" is merely redundant once
///   every code is user-facing, not false. So an empty set forbids only the
///   named exception, and a future paydown that empties the set is not blocked.
pub(super) fn claim_is_consistent(
    withheld: &[(&str, &str)],
    level: ClaimStrictness,
    text: &str,
) -> bool {
    let text = normalize_whitespace(text);
    if withheld.is_empty() {
        return level != ClaimStrictness::Explains || !describes_an_exception(&text);
    }
    if exhaustiveness_claims(&text)
        .iter()
        .any(|qualified| !qualified)
    {
        return false;
    }
    level != ClaimStrictness::Explains || names_a_withheld_entry(&text, withheld)
}

/// Collapse every whitespace run to one space.
///
/// clap wraps long help to terminal width, so a phrase that straddles a wrap
/// point would otherwise fail to match — spuriously, and for a reason that
/// looks nothing like the cause.
pub(super) fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One `true`/`false` per claim of exhaustiveness in `text`: whether that claim
/// carries [`QUALIFIER`].
///
/// Separate from the verdict so it can be asserted directly. A scanner that
/// silently matched nothing would make every consistency check below vacuously
/// true, which is the same shape of failure the whole ADR is about.
pub(super) fn exhaustiveness_claims(text: &str) -> Vec<bool> {
    let words: Vec<&str> = text.split(' ').collect();
    let mut claims = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if !TOTALITY_WORDS.contains(&bare_word(word).as_str()) {
            continue;
        }
        let window = &words[i + 1..words.len().min(i + 1 + NOUN_WINDOW)];
        let noun_at = window
            .iter()
            .position(|w| LISTING_NOUNS.contains(&bare_word(w).as_str()));
        let elided_noun = i > 0 && LISTING_VERBS.contains(&bare_word(words[i - 1]).as_str());
        if noun_at.is_none() && !elided_noun {
            continue;
        }
        let scanned = noun_at.map_or(window.len(), |at| at + 1);
        claims.push(window[..scanned].iter().any(|w| bare_word(w) == QUALIFIER));
    }
    claims
}

/// A word with its surrounding punctuation and case removed, so `"(E9998)"`,
/// `` "`code`," `` and `"all."` compare as themselves. Interior hyphens stay:
/// [`QUALIFIER`] is one word.
fn bare_word(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
        .to_lowercase()
}

/// Whether `text` marks anything at all as held back from the listing.
fn marks_an_exclusion(text: &str) -> bool {
    let lower = text.to_lowercase();
    EXCLUSION_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Whether `text` describes *some* entry as withheld — an exclusion marker plus
/// a code-shaped token for it to attach to.
///
/// Used only when the withheld set is empty, where naming any exception at all
/// is the falsehood, and there is no set to look the name up in.
pub(super) fn describes_an_exception(text: &str) -> bool {
    marks_an_exclusion(text)
        && text
            .split(' ')
            .any(|w| error_catalogue::is_code_shaped_for_test(&bare_word(w)))
}

/// Whether `text` names one of `withheld` **as** withheld.
///
/// The exclusion marker is required: a code mentioned somewhere else in the
/// help — in an example, say — tells a reader nothing about what the listing
/// leaves out.
pub(super) fn names_a_withheld_entry(text: &str, withheld: &[(&str, &str)]) -> bool {
    let lower = text.to_lowercase();
    marks_an_exclusion(text)
        && withheld.iter().any(|(kind, code)| {
            lower.contains(&kind.to_lowercase()) || lower.contains(&code.to_lowercase())
        })
}
