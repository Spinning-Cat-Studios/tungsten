//! `tungsten doctor suggest-tools` — map error descriptions to diagnostic commands.
//!
//! A static pattern-matching registry that maps error keywords/signals to
//! ranked diagnostic tool suggestions. Designed for AI agent consumption
//! via `--json`. See ADR 21.4.26d for design rationale.

use std::process::ExitCode;

use serde::Serialize;

mod patterns;
#[cfg(test)]
mod tests;

use patterns::PATTERNS;

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

/// Match an error description against the pattern registry and return
/// scored suggestions sorted by relevance (highest first).
///
/// If a sidecar experience store is available, learned relevance
/// adjustments are merged with static weights (see ADR 21.4.26f §2.4).
pub(crate) fn match_suggestions(description: &str) -> Vec<ScoredSuggestion> {
    let desc_lower = description.to_lowercase();

    // Try to open the sidecar store for learned relevance adjustments.
    // Failure is not an error — we fall back to static weights.
    let sidecar_entries = crate::sidecar::ExperienceStore::open_default()
        .ok()
        .and_then(|store| store.get_relevance_for_pattern(description).ok());

    // Score each pattern by counting keyword matches
    let mut scored: Vec<(f32, &ErrorPattern)> = Vec::new();
    for pattern in PATTERNS {
        let match_count = pattern
            .keywords
            .iter()
            .filter(|kw| desc_lower.contains(*kw))
            .count();
        if match_count > 0 {
            // Pattern score = number of keyword matches (more matches = more relevant)
            let pattern_score = match_count as f32;
            scored.push((pattern_score, pattern));
        }
    }

    // Sort patterns by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Collect suggestions, deduplicating by command name.
    // If the same command appears in multiple patterns, keep the higher relevance.
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<ScoredSuggestion> = Vec::new();

    for (pattern_score, pattern) in &scored {
        for suggestion in pattern.suggestions {
            if seen.insert(suggestion.command) {
                // Scale relevance by pattern match quality (cap at 1.0)
                let mut adjusted = (suggestion.relevance * pattern_score).min(1.0);

                // Apply sidecar learned adjustment if available
                if let Some(ref entries) = sidecar_entries {
                    if let Some(entry) = entries.get(suggestion.command) {
                        adjusted = crate::sidecar::adjust_relevance(adjusted, entry);
                    }
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
// Output
// ═══════════════════════════════════════════════════════════════════════

fn print_human(suggestions: &[ScoredSuggestion]) {
    if suggestions.is_empty() {
        println!("No matching diagnostic tools found for this description.");
        println!();
        println!("Tip: try keywords like 'sigsegv', 'type mismatch', 'stack overflow',");
        println!("     'encoding', 'mutual recursion', or 'elaboration error'.");
        return;
    }

    println!("Suggested diagnostic commands (most relevant first):");
    println!();
    for (i, s) in suggestions.iter().enumerate() {
        println!("  {}. {}  [cost {}]", i + 1, s.command, s.cost);
        println!("     Reason: {}", s.reason);
        println!();
    }
}

fn print_json(suggestions: &[ScoredSuggestion]) {
    let json = serde_json::to_string_pretty(suggestions).unwrap_or_else(|_| "[]".to_string());
    println!("{json}");
}

// ═══════════════════════════════════════════════════════════════════════
// Command Entry Point
// ═══════════════════════════════════════════════════════════════════════

pub(crate) fn cmd_suggest_tools(description: &str, json: bool) -> ExitCode {
    // Try sidecar process first (ADR 21.4.26g §2.4)
    #[cfg(unix)]
    if let Some(items_json) = try_suggest_via_socket(description) {
        if json {
            println!("{items_json}");
        } else {
            print_socket_suggestions(&items_json);
        }
        return ExitCode::SUCCESS;
    }

    // Fall back to direct matching (reads LMDB if available)
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
#[cfg(unix)]
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

/// Display suggestions received from the sidecar socket in human-readable form.
#[cfg(unix)]
fn print_socket_suggestions(items_json: &str) {
    let items: Vec<serde_json::Value> = match serde_json::from_str(items_json) {
        Ok(v) => v,
        Err(_) => {
            println!("{items_json}");
            return;
        }
    };

    if items.is_empty() {
        println!("No matching diagnostic tools found for this description.");
        println!();
        println!("Tip: try keywords like 'sigsegv', 'type mismatch', 'stack overflow',");
        println!("     'encoding', 'mutual recursion', or 'elaboration error'.");
        return;
    }

    println!("Suggested diagnostic commands (most relevant first):");
    println!();
    for (i, item) in items.iter().enumerate() {
        let cmd = item.get("command").and_then(|v| v.as_str()).unwrap_or("?");
        let cost = item.get("cost").and_then(|v| v.as_u64()).unwrap_or(0);
        let reason = item.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
        println!("  {}. {}  [cost {}]", i + 1, cmd, cost);
        println!("     Reason: {}", reason);
        println!();
    }
}
