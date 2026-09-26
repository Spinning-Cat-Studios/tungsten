//! What `suggest-tools` prints: the ranked listing, its JSON form, and the
//! answer when nothing matched.
//!
//! Split from the parent module when ADR 4.9.26d pushed it against its size
//! limit, along the seam the file already declared with its own `Output`
//! banner: everything here renders an answer someone reads, and nothing here
//! decides what the answer is. Both render paths — the direct one and the
//! sidecar socket's — live together on purpose, because their one shared
//! obligation is to say the same thing when there is nothing to say.
//!
//! **Each renderer returns its text; the `print_*` wrappers only print it.**
//! Rendering into a `String` is what makes the answer assertable at all: an
//! empty result must still SAY something (AC 6), and the wording is shared
//! across two paths that would otherwise drift silently. The wrappers stay
//! one line each so nothing decidable lives behind stdout.

use super::ScoredSuggestion;

/// The example queries the "nothing matched" answer offers.
///
/// Every one is **symptom-shaped**: what a reader saw, not what caused it. The
/// list this replaced was the opposite — `sigsegv`, `encoding`, `mutual
/// recursion`, `miscompile` — which taught the one lesson the empty answer must
/// not teach, that you have to know the diagnosis before the tool will help
/// (ADR 4.9.26d D4). Each is asserted to match something in
/// [`super::tests`]: an example that returns nothing is worse than no example.
pub(super) const NO_MATCH_EXAMPLES: &[&str] = &[
    "the binary dies immediately when i run it",
    "the answer is wrong but nothing errors",
    "reading a field gives back the wrong value",
    "the compiler has printed nothing for ten minutes",
    "my recursive function is rejected",
];

/// The heading above a non-empty listing — the phrase that tells a reader the
/// order means something, and the one an empty answer must never carry.
const LISTING_HEADING: &str = "Suggested diagnostic commands (most relevant first):";

/// The whole "nothing matched" answer, as text.
pub(super) fn no_match_report() -> String {
    let mut report = String::from("No matching diagnostic tools found for this description.\n\n");
    report.push_str("Tip: describe what you SAW, not what you think caused it — e.g.\n");
    for example in NO_MATCH_EXAMPLES {
        report.push_str(&format!("     '{example}'\n"));
    }
    report
}

/// One numbered entry of a listing, in the shape both render paths share.
fn listing_entry(position: usize, command: &str, cost: u64, reason: &str) -> String {
    format!("  {position}. {command}  [cost {cost}]\n     Reason: {reason}\n\n")
}

/// The ranked listing for a set of scored suggestions, or the no-match answer
/// when there are none.
pub(super) fn human_report(suggestions: &[ScoredSuggestion]) -> String {
    if suggestions.is_empty() {
        return no_match_report();
    }

    let mut report = format!("{LISTING_HEADING}\n\n");
    for (i, s) in suggestions.iter().enumerate() {
        report.push_str(&listing_entry(
            i + 1,
            s.command,
            u64::from(s.cost),
            s.reason,
        ));
    }
    report
}

/// The JSON form. A serialization failure renders as the empty array rather
/// than as nothing at all, so a consumer parsing stdout always has a document.
pub(super) fn json_report(suggestions: &[ScoredSuggestion]) -> String {
    serde_json::to_string_pretty(suggestions).unwrap_or_else(|_| "[]".to_string())
}

/// The same listing, rendered from the sidecar socket's JSON items.
///
/// Unparseable input is echoed verbatim: the socket's answer is the sidecar's
/// to make, and swallowing it would leave the user with nothing to report.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(super) fn socket_report(items_json: &str) -> String {
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(items_json) else {
        return format!("{items_json}\n");
    };

    if items.is_empty() {
        return no_match_report();
    }

    let mut report = format!("{LISTING_HEADING}\n\n");
    for (i, item) in items.iter().enumerate() {
        let command = item.get("command").and_then(|v| v.as_str()).unwrap_or("?");
        let cost = item.get("cost").and_then(|v| v.as_u64()).unwrap_or(0);
        let reason = item.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
        report.push_str(&listing_entry(i + 1, command, cost, reason));
    }
    report
}

pub(super) fn print_human(suggestions: &[ScoredSuggestion]) {
    print!("{}", human_report(suggestions));
}

pub(super) fn print_json(suggestions: &[ScoredSuggestion]) {
    println!("{}", json_report(suggestions));
}

/// Display suggestions received from the sidecar socket in human-readable form.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(super) fn print_socket_suggestions(items_json: &str) {
    print!("{}", socket_report(items_json));
}

#[cfg(test)]
mod tests;
