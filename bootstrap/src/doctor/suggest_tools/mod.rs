//! `tungsten doctor suggest-tools` — map error descriptions to diagnostic commands.
//!
//! A static pattern-matching registry that maps error keywords/signals to
//! ranked diagnostic tool suggestions. Designed for AI agent consumption
//! via `--json`. See ADR 21.4.26d for design rationale.

use std::collections::HashMap;
use std::process::ExitCode;

use serde::Serialize;

#[cfg(test)]
mod keyword_hygiene_tests;
#[cfg(test)]
mod matcher_property_tests;
mod output;
mod patterns;
#[cfg(test)]
mod symptom_reachability_tests;
#[cfg(test)]
mod termination_tests;
#[cfg(test)]
mod tests;

#[cfg(all(unix, not(target_arch = "wasm32")))]
use output::print_socket_suggestions;
use output::{print_human, print_json};
use patterns::all_patterns;

// ═══════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════

/// A recommended diagnostic command for a matched error pattern.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ToolSuggestion {
    pub command: &'static str,
    pub cost: u8,
    pub reason: &'static str,
    /// Base relevance weight (0.0–1.0). Higher = more relevant to the pattern.
    pub relevance: f32,
}

/// An error pattern that maps keywords/signals to tool suggestions.
#[derive(Debug)]
struct ErrorPattern {
    /// Category name for display (e.g., "segfault", "type mismatch")
    category: &'static str,
    /// Keywords that match this pattern (lowercase). Any match counts.
    keywords: &'static [&'static str],
    /// Diagnostic commands suggested for this pattern.
    suggestions: &'static [ToolSuggestion],
}

/// A scored suggestion returned by the matching engine.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScoredSuggestion {
    pub command: &'static str,
    pub cost: u8,
    pub reason: &'static str,
    pub relevance: f32,
}

// ═══════════════════════════════════════════════════════════════════════
// Matching Engine
// ═══════════════════════════════════════════════════════════════════════

/// Score every registry pattern against a lowercased error description by
/// keyword-overlap count, keeping only matches, sorted by score descending
/// (ties preserve registry order — `sort_by` is stable). Shared by the
/// suggestion engine and the sidecar relevance-key derivation so both agree on
/// which error *class* a description belongs to.
fn scored_patterns(desc_lower: &str) -> Vec<(f32, &'static ErrorPattern)> {
    let mut scored: Vec<(f32, &ErrorPattern)> = Vec::new();
    for pattern in all_patterns() {
        let match_count = pattern
            .keywords
            .iter()
            .filter(|kw| desc_lower.contains(*kw))
            .count();
        if match_count > 0 {
            // Pattern score = number of keyword matches (more matches = more relevant)
            scored.push((match_count as f32, pattern));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// The registry *category* of the top-scoring pattern for an error description,
/// or `None` when nothing matches. This is the normalized key the sidecar
/// learns relevance under (ADR 23.7.26e D2), replacing the verbatim free-text
/// description so recurring error *classes* accumulate learning.
pub(crate) fn top_category(description: &str) -> Option<&'static str> {
    scored_patterns(&description.to_lowercase())
        .first()
        .map(|(_, p)| p.category)
}

/// The cost tier a suggested `command` carries in the registry, or `None` if it
/// is not a known suggestion. Costs are consistent across patterns, so the
/// first match wins.
pub(crate) fn command_cost(command: &str) -> Option<u8> {
    all_patterns()
        .flat_map(|p| p.suggestions.iter())
        .find(|s| s.command == command)
        .map(|s| s.cost)
}

/// The normalized sidecar-relevance context for one (error, command) pair: the
/// error *class* to key learned relevance under, and the command's known cost
/// tier. Both are derived from the static registry so every sidecar write site
/// and the `suggest-tools` read path agree on the key (ADR 23.7.26e D2).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RelevanceContext {
    /// Category of the top-scoring pattern for the error text, or the verbatim
    /// text when nothing matches (unmatched text yields no suggestions, so its
    /// key is never read back).
    pub category: String,
    /// The command's registry cost tier, or 0 when the command is unknown.
    pub cost: u8,
}

/// Derive the [`RelevanceContext`] for an (error, command) pair — the single
/// shared helper routed through every sidecar write site (ADR 23.7.26e D2), so
/// the key/cost derivation cannot silently diverge between paths.
pub(crate) fn relevance_context(description: &str, command: &str) -> RelevanceContext {
    RelevanceContext {
        category: top_category(description)
            .map(str::to_string)
            .unwrap_or_else(|| description.to_string()),
        cost: command_cost(command).unwrap_or(0),
    }
}

/// Match an error description against the pattern registry and return scored
/// suggestions sorted by relevance (highest first).
///
/// When the sidecar is enabled (ADR 23.7.26e D1) and learned relevance exists
/// for the description's error class, those adjustments are merged with the
/// static weights; otherwise the static ranking is returned unchanged.
pub(crate) fn match_suggestions(description: &str) -> Vec<ScoredSuggestion> {
    // Learned relevance is keyed by the error *class* (top category) and only
    // consulted when the sidecar is activated for this repo.
    let learned = if crate::sidecar::sidecar_enabled() {
        top_category(description).and_then(stored_relevance_for_category)
    } else {
        None
    };
    match_suggestions_with_entries(description, learned.as_ref())
}

/// Read the learned relevance entries stored under `category`, or `None` when
/// the store is unavailable or has nothing for it.
///
/// Split out for one reason only: `ExperienceStore` is LMDB-backed and compiled
/// out on `wasm32` (ADR 28.7.26a §2.1), so this is the narrowest possible
/// expression to put behind a `#[cfg]`. Everything above it — the enablement
/// check, the category derivation, the merge — stays target-invariant and
/// stays inside [`match_suggestions`], where the existing tests reach it.
#[cfg(not(target_arch = "wasm32"))]
fn stored_relevance_for_category(
    category: &str,
) -> Option<HashMap<String, crate::sidecar::RelevanceEntry>> {
    crate::sidecar::ExperienceStore::open_default()
        .ok()
        .and_then(|store| store.get_relevance_for_pattern(category).ok())
}

/// There is no store to read on `wasm32`.
///
/// `None` is the same answer the arm above gives when the store cannot be
/// opened, so callers take an already-exercised path and `suggest-tools` keeps
/// its static ranking rather than disappearing.
#[cfg(target_arch = "wasm32")]
fn stored_relevance_for_category(
    _category: &str,
) -> Option<HashMap<String, crate::sidecar::RelevanceEntry>> {
    None
}

/// Pure scoring + optional learned-relevance adjustment. `learned` maps a
/// command → its `RelevanceEntry` for the description's category; `None` yields
/// the static ranking. Split from [`match_suggestions`] so the learning path is
/// unit-testable without touching the on-disk store (ADR 23.7.26e).
pub(crate) fn match_suggestions_with_entries(
    description: &str,
    learned: Option<&HashMap<String, crate::sidecar::RelevanceEntry>>,
) -> Vec<ScoredSuggestion> {
    let scored = scored_patterns(&description.to_lowercase());

    // Collect suggestions, deduplicating by command name (keep the first, i.e.
    // highest-scoring-pattern, occurrence).
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<ScoredSuggestion> = Vec::new();

    for (pattern_score, pattern) in &scored {
        for suggestion in pattern.suggestions {
            if seen.insert(suggestion.command) {
                // Scale relevance by pattern match quality (cap at 1.0).
                //
                // The cap makes `relevance` INERT for any query that matches
                // more than one keyword, which is the common case: with
                // `pattern_score` an integer keyword count, every suggestion in
                // a two-keyword match computes `>= 1.0` for any relevance above
                // 0.5 and all of them clamp to exactly 1.0. The sort below is
                // stable, so ties keep insertion order and the ranking a user
                // sees is DECLARATION ORDER in the pattern table.
                //
                // Measured, not inferred: dropping one entry from 0.95 to 0.50
                // — below a sibling at 0.90 — left the shipped output for
                // `'cannot prove termination'` byte-identical, while swapping
                // the two entries' positions changed it (ADR 12.8.26a's
                // `/check-adr` pass). So relevance separates suggestions only
                // for single-keyword matches; to re-rank anything else, move
                // the entry. Fixing this means renormalizing `pattern_score`
                // rather than clamping, which reorders every category's output
                // and is its own change.
                let mut adjusted = (suggestion.relevance * pattern_score).min(1.0);

                // Apply sidecar learned adjustment if available
                if let Some(entry) = learned.and_then(|l| l.get(suggestion.command)) {
                    adjusted = crate::sidecar::adjust_relevance(adjusted, entry);
                }

                results.push(ScoredSuggestion {
                    command: suggestion.command,
                    cost: suggestion.cost,
                    reason: suggestion.reason,
                    relevance: adjusted,
                });
            }
        }
    }

    // Final sort by relevance descending
    results.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

// ═══════════════════════════════════════════════════════════════════════
// Command Entry Point
// ═══════════════════════════════════════════════════════════════════════

pub(crate) fn cmd_suggest_tools(description: &str, json: bool) -> ExitCode {
    // Try sidecar process first (ADR 21.4.26g §2.4) — but only when the sidecar
    // is activated (ADR 23.7.26e D1). Gating only the direct path would leak: a
    // live `serve` process applies the learned adjustment server-side, so the
    // socket-first path must be bypassed when disabled.
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    if crate::sidecar::sidecar_enabled() {
        if let Some(items_json) = try_suggest_via_socket(description) {
            if json {
                println!("{items_json}");
            } else {
                print_socket_suggestions(&items_json);
            }
            return ExitCode::SUCCESS;
        }
    }

    // Fall back to direct matching (reads LMDB only when enabled)
    let suggestions = match_suggestions(description);

    if json {
        print_json(&suggestions);
    } else {
        print_human(&suggestions);
    }

    ExitCode::SUCCESS
}

/// Query the sidecar process via Unix domain socket.
/// Returns the raw JSON array of suggestion items, or None on failure.
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn try_suggest_via_socket(description: &str) -> Option<String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let store_dir = crate::sidecar::store::default_store_dir().ok()?;
    let socket_path = store_dir.join("sidecar.sock");

    if !socket_path.exists() {
        return None;
    }

    let mut stream = UnixStream::connect(&socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .ok()?;

    let request = serde_json::json!({
        "v": 1,
        "type": "suggest",
        "error": description
    });
    writeln!(stream, "{request}").ok()?;
    stream.flush().ok()?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok()?;

    let parsed: serde_json::Value = serde_json::from_str(&response).ok()?;
    let items = parsed.get("items")?;
    serde_json::to_string_pretty(items).ok()
}
