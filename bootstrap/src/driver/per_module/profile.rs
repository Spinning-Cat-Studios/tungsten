//! Per-phase profiling for cold-cache elaboration (ADR 11.5.26b §P0).
//!
//! Enabled via `TUNGSTEN_ELAB_PROFILE=1`. Emits a timing summary table
//! to stderr after elaboration completes, breaking down each phase and
//! per-module sub-phases within Body Elaboration.

use std::time::Duration;

/// Whether elaboration profiling is enabled.
pub(super) fn is_enabled() -> bool {
    std::env::var("TUNGSTEN_ELAB_PROFILE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Accumulated per-module timing within Body Elaboration.
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
    pub(super) stub_registration: Duration,
    pub(super) signature_collection: Duration,
    pub(super) body_elaboration_total: Duration,
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
    /// Phase-level durations (stub_registration, signature_collection, body_elaboration_total) are NOT merged
    /// — they are set once at the top level.
    pub(super) fn merge_from(&mut self, other: &ElabProfile) {
        self.modules.extend(other.modules.iter().cloned());
    }

    /// The three phase durations, summed. This is the denominator every
    /// percentage below is a share of — **not** wall-clock (ADR 5.8.26d §6.2).
    pub(super) fn phase_total(&self) -> Duration {
        self.stub_registration + self.signature_collection + self.body_elaboration_total
    }

    /// Body Elaboration time not attributed to any sub-row.
    ///
    /// The sub-rows sum only over *fresh* modules, so this covers cache-hit
    /// reads, per-module env setup and the walk itself. Reporting it is what
    /// makes the breakdown add up: measured on `src/compiler/test_compare_result.tg`
    /// the phase was 34.8 s against 25.4 s of sub-rows, and the missing ~9.4 s
    /// had to be derived by hand (ADR 5.8.26d).
    pub(super) fn unattributed_body_time(&self) -> Duration {
        let attributed: Duration = self
            .modules
            .iter()
            .filter(|m| !m.cache_hit)
            .map(|m| m.collection + m.body + m.cache_write)
            .sum();
        self.body_elaboration_total.saturating_sub(attributed)
    }

    /// Emit the profiling summary to stderr.
    pub(super) fn emit(&self) {
        let total = self.phase_total();

        eprintln!();
        eprintln!("=== Elaboration Profile (ADR 11.5.26b P0) ===");
        eprintln!();
        emit_row("Stub Registration", self.stub_registration, total);
        emit_row(
            "Signature Collection (combined)",
            self.signature_collection,
            total,
        );
        emit_row(
            "Body Elaboration (per-module)",
            self.body_elaboration_total,
            total,
        );
        eprintln!("  ────────────────────────────────────────");
        emit_row("Total (elaboration only)", total, total);
        // Shares are of THIS total, not of wall-clock: parse, module-graph
        // construction and run/test execution happen outside the elaborator and
        // are not measured here. On src/compiler they are ~3.5 s of a ~38.6 s
        // `test` run, so a reader comparing this Total against `time` will find
        // a gap and should not mistake it for a missing phase (ADR 5.8.26d §6.2).
        eprintln!(
            "  (shares are of this total; wall-clock also includes parse,\n   \
             module-graph build and any run/test execution)"
        );

        // Body Elaboration breakdown
        let cache_hits: Vec<_> = self.modules.iter().filter(|m| m.cache_hit).collect();
        let fresh: Vec<_> = self.modules.iter().filter(|m| !m.cache_hit).collect();
        let total_collection: Duration = fresh.iter().map(|m| m.collection).sum();
        let total_body: Duration = fresh.iter().map(|m| m.body).sum();
        let total_cache_write: Duration = fresh.iter().map(|m| m.cache_write).sum();

        eprintln!();
        eprintln!(
            "  Body Elaboration breakdown ({} modules, {} cache hits, {} fresh):",
            self.modules.len(),
            cache_hits.len(),
            fresh.len()
        );
        emit_row(
            "    Collection (types)",
            total_collection,
            self.body_elaboration_total,
        );
        emit_row(
            "    Body elab (values + bodies)",
            total_body,
            self.body_elaboration_total,
        );
        emit_row(
            "    Cache writes",
            total_cache_write,
            self.body_elaboration_total,
        );
        // Without this the sub-rows silently fail to add up to their phase, and
        // the difference has to be derived by hand (ADR 5.8.26d §6.2).
        emit_row(
            "    Unattributed (walk, cache reads)",
            self.unattributed_body_time(),
            self.body_elaboration_total,
        );

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

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(path: &str, collection: u64, body: u64, write: u64, hit: bool) -> ModuleTiming {
        ModuleTiming {
            path: path.to_string(),
            collection: Duration::from_millis(collection),
            body: Duration::from_millis(body),
            cache_write: Duration::from_millis(write),
            cache_hit: hit,
        }
    }

    fn profile_with(body_total_ms: u64, modules: Vec<ModuleTiming>) -> ElabProfile {
        ElabProfile {
            stub_registration: Duration::from_millis(1),
            signature_collection: Duration::from_millis(10),
            body_elaboration_total: Duration::from_millis(body_total_ms),
            modules,
        }
    }

    #[test]
    fn phase_total_sums_the_three_phases_and_nothing_else() {
        let p = profile_with(100, vec![]);
        assert_eq!(p.phase_total(), Duration::from_millis(111));
    }

    #[test]
    fn unattributed_is_the_phase_minus_its_sub_rows() {
        // 100 ms phase; one fresh module accounting for 30+20+5 = 55 ms.
        let p = profile_with(100, vec![timing("a", 30, 20, 5, false)]);
        assert_eq!(p.unattributed_body_time(), Duration::from_millis(45));
    }

    #[test]
    fn cache_hit_modules_contribute_nothing_attributed_so_all_time_is_unattributed() {
        // A hit module's collection/body are not summed by the breakdown, so
        // its read cost lands here rather than vanishing.
        let p = profile_with(100, vec![timing("a", 30, 20, 5, true)]);
        assert_eq!(p.unattributed_body_time(), Duration::from_millis(100));
    }

    #[test]
    fn unattributed_saturates_rather_than_underflowing() {
        // Sub-rows can exceed the phase total under parallel elaboration, where
        // per-worker times overlap in wall-clock. Duration subtraction would panic.
        let p = profile_with(10, vec![timing("a", 30, 20, 5, false)]);
        assert_eq!(p.unattributed_body_time(), Duration::ZERO);
    }

    #[test]
    fn sub_rows_plus_unattributed_reconstruct_the_phase() {
        // The property the row exists to guarantee: the breakdown adds up.
        let modules = vec![timing("a", 30, 20, 5, false), timing("b", 10, 5, 1, false)];
        let p = profile_with(100, modules);
        let attributed: Duration = p
            .modules
            .iter()
            .map(|m| m.collection + m.body + m.cache_write)
            .sum();
        assert_eq!(
            attributed + p.unattributed_body_time(),
            p.body_elaboration_total
        );
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
