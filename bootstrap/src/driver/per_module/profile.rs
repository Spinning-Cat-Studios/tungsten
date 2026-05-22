//! Per-phase profiling for cold-cache elaboration (ADR 11.5.26b §P0).
//!
//! Enabled via `TUNGSTEN_ELAB_PROFILE=1`. Emits a timing summary table
//! to stderr after elaboration completes, breaking down each phase and
//! per-module sub-phases within Phase B.

use std::time::Duration;

/// Whether elaboration profiling is enabled.
pub(super) fn is_enabled() -> bool {
    std::env::var("TUNGSTEN_ELAB_PROFILE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Accumulated per-module timing within Phase B.
#[derive(Debug, Clone)]
pub(super) struct ModuleTiming {
    pub(super) path: String,
    pub(super) collection: Duration,
    pub(super) body: Duration,
    pub(super) cache_write: Duration,
    pub(super) cache_hit: bool,
}

/// Full elaboration profile collected across all phases.
#[derive(Debug, Default)]
pub(super) struct ElabProfile {
    pub(super) phase_a: Duration,
    pub(super) phase_a5: Duration,
    pub(super) phase_b_total: Duration,
    pub(super) modules: Vec<ModuleTiming>,
}

impl ElabProfile {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Record a per-module timing entry.
    pub(super) fn record_module(&mut self, timing: ModuleTiming) {
        self.modules.push(timing);
    }

    /// Merge another profile's module timings into this one (ADR 11.5.26b §P5).
    ///
    /// Used to collect per-worker profiles after parallel level elaboration.
    /// Phase-level durations (phase_a, phase_a5, phase_b_total) are NOT merged
    /// — they are set once at the top level.
    pub(super) fn merge_from(&mut self, other: &ElabProfile) {
        self.modules.extend(other.modules.iter().cloned());
    }

    /// Emit the profiling summary to stderr.
    pub(super) fn emit(&self) {
        let total = self.phase_a + self.phase_a5 + self.phase_b_total;

        eprintln!();
        eprintln!("=== Elaboration Profile (ADR 11.5.26b P0) ===");
        eprintln!();
        emit_row("Phase A (stubs)", self.phase_a, total);
        emit_row("Phase A.5 (combined)", self.phase_a5, total);
        emit_row("Phase B (per-module)", self.phase_b_total, total);
        eprintln!("  ────────────────────────────────────────");
        emit_row("Total", total, total);

        // Phase B breakdown
        let cache_hits: Vec<_> = self.modules.iter().filter(|m| m.cache_hit).collect();
        let fresh: Vec<_> = self.modules.iter().filter(|m| !m.cache_hit).collect();
        let total_collection: Duration = fresh.iter().map(|m| m.collection).sum();
        let total_body: Duration = fresh.iter().map(|m| m.body).sum();
        let total_cache_write: Duration = fresh.iter().map(|m| m.cache_write).sum();

        eprintln!();
        eprintln!(
            "  Phase B breakdown ({} modules, {} cache hits, {} fresh):",
            self.modules.len(),
            cache_hits.len(),
            fresh.len()
        );
        emit_row(
            "    Collection (Phase 1)",
            total_collection,
            self.phase_b_total,
        );
        emit_row("    Body elab (Phase 2)", total_body, self.phase_b_total);
        emit_row("    Cache writes", total_cache_write, self.phase_b_total);

        // Top 10 slowest modules
        if fresh.len() > 1 {
            let mut by_total: Vec<_> = fresh
                .iter()
                .map(|m| (m.path.as_str(), m.collection + m.body))
                .collect();
            by_total.sort_by(|a, b| b.1.cmp(&a.1));
            let top = by_total.iter().take(10);

            eprintln!();
            eprintln!("  Top modules by elaboration time:");
            for (path, dur) in top {
                eprintln!("    {:>7.1?}  {}", dur, path);
            }
        }

        eprintln!();
    }
}

fn emit_row(label: &str, dur: Duration, total: Duration) {
    let pct = if total.as_nanos() > 0 {
        dur.as_nanos() as f64 / total.as_nanos() as f64 * 100.0
    } else {
        0.0
    };
    eprintln!("  {:<35} {:>7.1?}  ({:>5.1}%)", label, dur, pct);
}
