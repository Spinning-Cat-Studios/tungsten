//! Relevance tuning algorithm for the experience store.
//!
//! Adjusts static registry weights based on observed success rates.
//! See ADR 21.4.26f §2.4 for the algorithm specification.

use serde::{Deserialize, Serialize};

/// Minimum observations before adjusting from static weight.
pub const MIN_SAMPLES: u32 = 5;

/// Maximum adjustment magnitude (±0.3).
pub const MAX_BOOST: f32 = 0.3;

/// Per-(pattern, command) observation counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelevanceEntry {
    pub shown_count: u32,
    pub helped_count: u32,
}

impl RelevanceEntry {
    /// Compute the success rate, or `None` if below the sample threshold.
    pub fn success_rate(&self) -> Option<f32> {
        if self.shown_count >= MIN_SAMPLES {
            Some(self.helped_count as f32 / self.shown_count as f32)
        } else {
            None
        }
    }
}

/// Compute the adjusted relevance for a (pattern, command) pair.
///
/// Returns the `base_relevance` unchanged if there aren't enough samples.
/// Otherwise, adjusts by `(success_rate - 0.5) × MAX_BOOST`, clamped to `[0.1, 1.0]`.
pub fn adjust_relevance(base_relevance: f32, entry: &RelevanceEntry) -> f32 {
    match entry.success_rate() {
        Some(rate) => {
            let adjustment = (rate - 0.5) * MAX_BOOST;
            (base_relevance + adjustment).clamp(0.1, 1.0)
        }
        None => base_relevance,
    }
}
