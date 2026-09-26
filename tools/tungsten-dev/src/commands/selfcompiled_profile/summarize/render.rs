//! Render parsed marker snapshots into the per-module summary table, plus the
//! opt-in `--by-class` (per-class deltas + share) and `--growth` (walk-order
//! index trend) attributions (ADR 24.7.26a).

use super::{MarkerSnapshot, CLASS_SHORT, MODULE_MARKER_PREFIX, NUM_CLASSES};

const MB: u64 = 1024 * 1024;

/// Window size (modules) for the `--growth` first/last means. 30 mirrors the
/// window ADR 23.7.26d used to expose the O(M²) index-proportional ramp on a
/// 213-module walk (first-30 15.2 MB → last-30 164.0 MB).
const GROWTH_WINDOW: usize = 30;

/// Tolerance for calling the mean ratio "∝ index" — the mean ratio is accepted
/// as super-linear if it reaches within `GROWTH_BAND` *below* the
/// window-centroid index ratio (ADR 24.7.26a §2.3, proposed ±25%).
const GROWTH_BAND: f64 = 0.25;

/// Per-module growth, attributed per the marker-at-start convention.
struct ModuleDelta {
    module: String,
    rss_mb: u64,
    terms_mb: u64,
    types_mb: u64,
    selfhost_alloc_mb: u64,
    /// Per-class self-host alloc delta (MB), `CLASS_SHORT` order.
    class_mb: [u64; NUM_CLASSES],
}

/// Render the per-module delta table + terminal totals. `by_class` adds
/// per-class delta columns and a terminal share table; `growth` adds a
/// walk-order index-trend verdict. With both flags off the output is
/// byte-identical to the pre-24.7.26a summarizer.
pub fn render_summary(
    snapshots: &[MarkerSnapshot],
    top: usize,
    by_class: bool,
    growth: bool,
) -> String {
    let mut deltas = compute_deltas(snapshots);
    let module_count = deltas.len();

    let mut out = format!(
        "== self-compiled heap profile: {} markers, {} module deltas ==\n",
        snapshots.len(),
        module_count
    );
    if let Some(last) = snapshots.last() {
        out.push_str(&terminal_line(last));
    }
    if growth {
        out.push_str(&render_growth(&deltas));
    }

    // Growth reads walk order; sort only afterwards for the size-ranked table.
    deltas.sort_by(|a, b| (b.rss_mb, b.selfhost_alloc_mb).cmp(&(a.rss_mb, a.selfhost_alloc_mb)));
    out.push_str(&render_table(&deltas, top, module_count, by_class));

    if by_class {
        if let Some(last) = snapshots.last() {
            out.push_str(&render_class_table(last));
        }
    }
    out
}

/// Build per-module deltas in walk order (before any sorting).
fn compute_deltas(snapshots: &[MarkerSnapshot]) -> Vec<ModuleDelta> {
    let mut deltas: Vec<ModuleDelta> = Vec::new();
    for pair in snapshots.windows(2) {
        let (earlier, later) = (&pair[0], &pair[1]);
        let Some(module) = earlier.label.strip_prefix(MODULE_MARKER_PREFIX) else {
            continue;
        };
        let (rss_mb, terms_mb, types_mb) = match (&earlier.arena, &later.arena) {
            (Some(a), Some(b)) => (
                b.vmrss_mb.saturating_sub(a.vmrss_mb),
                b.terms_deep_mb.saturating_sub(a.terms_deep_mb),
                b.types_deep_mb.saturating_sub(a.types_deep_mb),
            ),
            _ => (0, 0, 0),
        };
        let mut class_mb = [0u64; NUM_CLASSES];
        for (i, slot) in class_mb.iter_mut().enumerate() {
            *slot = later.by_class[i].saturating_sub(earlier.by_class[i]) / MB;
        }
        deltas.push(ModuleDelta {
            module: module.to_string(),
            rss_mb,
            terms_mb,
            types_mb,
            selfhost_alloc_mb: later
                .alloc_total_bytes
                .saturating_sub(earlier.alloc_total_bytes)
                / MB,
            class_mb,
        });
    }
    deltas
}

/// The `terminal: at ...` line reporting the final snapshot's cumulative state.
fn terminal_line(last: &MarkerSnapshot) -> String {
    let mut out = format!(
        "terminal: at `{}` — self-host alloc={}MB",
        last.label,
        last.alloc_total_bytes / MB
    );
    if let Some(a) = &last.arena {
        out.push_str(&format!(
            ", arena terms={}MB types={}MB slab={}MB, vmrss={}MB",
            a.terms_deep_mb, a.types_deep_mb, a.slab_mb, a.vmrss_mb
        ));
    }
    out.push('\n');
    out
}

/// The size-ranked per-module table (`by_class` appends class-delta columns).
fn render_table(deltas: &[ModuleDelta], top: usize, module_count: usize, by_class: bool) -> String {
    let mut out = format!(
        "top {} modules by RSS delta (a delta belongs to the module *starting* at its marker):\n",
        top.min(module_count)
    );
    for d in deltas.iter().take(top) {
        out.push_str(&format!(
            "  {:<52} dRSS={:>6}MB dTerms={:>6}MB dTypes={:>6}MB dSelfHost={:>5}MB",
            d.module, d.rss_mb, d.terms_mb, d.types_mb, d.selfhost_alloc_mb
        ));
        if by_class {
            out.push_str(&format!(
                " dMu={:>6}MB dEnv={:>6}MB dRef={:>6}MB dStr={:>6}MB",
                d.class_mb[0], d.class_mb[1], d.class_mb[2], d.class_mb[3]
            ));
        }
        out.push('\n');
    }
    out
}

/// Terminal per-class cumulative bytes + share of the terminal self-host alloc total
/// (the ADR 23.7.26d §1.5c "which class drives the ramp?" table).
fn render_class_table(last: &MarkerSnapshot) -> String {
    let total = last.alloc_total_bytes;
    let mut out = format!(
        "per-class cumulative (of terminal self-host alloc={}MB):\n",
        total / MB
    );
    for (i, name) in CLASS_SHORT.iter().enumerate() {
        let bytes = last.by_class[i];
        let share = if total > 0 {
            bytes as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        out.push_str(&format!(
            "  {:<6} {:>10}MB {:>6.2}%\n",
            name,
            bytes / MB,
            share
        ));
    }
    out
}

/// Walk-order index-trend attribution: does per-module cost grow with module
/// index (⇒ cumulative O(M²)) or stay flat (⇒ O(M))?
fn render_growth(deltas: &[ModuleDelta]) -> String {
    let n = deltas.len();
    if n < 2 {
        return format!("growth (walk order): {n} module delta(s) — too few to trend\n");
    }
    let window = GROWTH_WINDOW.min(n / 2).max(1);
    let first_mean = window_mean(&deltas[..window]);
    let last_mean = window_mean(&deltas[n - window..]);
    let mean_ratio = if first_mean > 0.0 {
        last_mean / first_mean
    } else {
        f64::INFINITY
    };
    // 1-based window centroids (avoids a zero denominator when window == 1).
    let first_centroid = (1.0 + window as f64) / 2.0;
    let last_centroid = n as f64 - (window as f64 - 1.0) / 2.0;
    let index_ratio = last_centroid / first_centroid;
    format!(
        "growth (walk order): first-{window} mean {first_mean:.1} MB → \
         last-{window} mean {last_mean:.1} MB = {mean_ratio:.2}×\n  \
         window-centroid index ratio ≈ {last_centroid:.0}/{first_centroid:.0} ≈ \
         {index_ratio:.1}×  ⇒  {}\n",
        growth_verdict(mean_ratio, index_ratio)
    )
}

/// Mean self-host alloc delta (MB) over a window of modules.
fn window_mean(deltas: &[ModuleDelta]) -> f64 {
    if deltas.is_empty() {
        return 0.0;
    }
    let sum: u64 = deltas.iter().map(|d| d.selfhost_alloc_mb).sum();
    sum as f64 / deltas.len() as f64
}

/// Classify per-module growth from the last/first mean ratio vs the
/// window-centroid index ratio. mean ∝ index ⇒ per-module cost ∝ index ⇒
/// cumulative O(M²); mean ≈ 1 ⇒ per-module cost ~constant ⇒ cumulative O(M);
/// in between ⇒ sub-index growth.
fn growth_verdict(mean_ratio: f64, index_ratio: f64) -> &'static str {
    if mean_ratio < 1.0 + GROWTH_BAND {
        "FLAT/LINEAR (per-module cost ~constant ⇒ O(M))"
    } else if mean_ratio >= index_ratio * (1.0 - GROWTH_BAND) {
        "SUPER-LINEAR (O(M²)-suspect)"
    } else {
        "SUB-LINEAR (grows, but slower than index ⇒ between O(M) and O(M²))"
    }
}

// Tests: render_tests.rs
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
