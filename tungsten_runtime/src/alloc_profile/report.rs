//! Report formatting for the allocation profiler.
//!
//! Three output forms:
//! - `print_report` — full end-of-run report (per-class summary, mu_alloc
//!   size histogram, top functions with dominant class)
//! - `print_interim` — compact snapshot printed every interval crossing,
//!   so OOM-killed runs still yield a ranking (ADR 2.7.26a)
//! - `print_marker` — one-line phase/module marker with cumulative totals
//!
//! Every form ends with the bump-arena fields (ADR 14.9.26b): the mode and
//! the calling thread's chunk/reserved/used/high-water counts, rendered by
//! `bump_fields` as `key=value` tokens whose keys collide with none of the
//! class keys the `selfcompiled-profile` parser locates by substring.

use super::{Profiler, CLASS_NAMES, MU_BUCKET_BOUNDS, NUM_CLASSES};
use crate::alloc::{current_arena_stats, ArenaStatsOut, MODE_BUMP};

/// Print the full allocation profile report to stderr.
pub(super) fn print_report(profiler: &Profiler) {
    if profiler.total_bytes == 0 {
        eprintln!("\n  Allocation Profile: no allocations recorded.");
        return;
    }

    let filter_name = resolve_filter(profiler);
    let mut indices = sorted_indices(profiler);

    if let Some(filter) = filter_name {
        indices.retain(|&idx| entry_name(profiler, idx) == filter);
    }

    let shown = indices.len().min(20);
    eprintln!();
    if let Some(filter) = filter_name {
        eprintln!("  Allocation Profile (filtered: {})", filter);
    } else {
        eprintln!("  Allocation Profile (top {} by bytes)", shown);
    }

    print_class_summary(profiler);
    print_mu_histogram(profiler);

    eprintln!("  {}", "─".repeat(78));
    eprintln!(
        "  {:40} {:>14} {:>8} {:>12}",
        "Function", "Bytes", "%", "Top class"
    );
    eprintln!("  {}", "─".repeat(78));

    for &idx in indices.iter().take(20) {
        let entry = &profiler.entries[idx];
        let pct = (entry.bytes as f64 / profiler.total_bytes as f64) * 100.0;
        eprintln!(
            "  {:40} {:>14} ({:>5.2}%) {:>12}",
            entry_name(profiler, idx),
            format_bytes(entry.bytes),
            pct,
            dominant_class(&entry.bytes_by_class)
        );
    }

    eprintln!("  {}", "─".repeat(78));
    eprintln!(
        "  {:40} {:>14} ({} calls)",
        "TOTAL",
        format_bytes(profiler.total_bytes),
        format_count(profiler.total_count)
    );
    eprintln!("  {}", bump_summary(&current_arena_stats()));
    eprintln!();
}

/// Print a compact interim snapshot (interval crossing).
pub(super) fn print_interim(profiler: &Profiler) {
    eprintln!(
        "  [alloc-profile] interim @ {} total ({} allocs)",
        format_bytes(profiler.total_bytes),
        format_count(profiler.total_count)
    );
    eprintln!("  [alloc-profile]   classes: {}", class_line(profiler));

    let indices = sorted_indices(profiler);
    for &idx in indices.iter().take(10) {
        let entry = &profiler.entries[idx];
        let pct = (entry.bytes as f64 / profiler.total_bytes as f64) * 100.0;
        eprintln!(
            "  [alloc-profile]   {:40} {:>14} ({:>5.2}%) {}",
            entry_name(profiler, idx),
            format_bytes(entry.bytes),
            pct,
            dominant_class(&entry.bytes_by_class)
        );
    }
}

/// Print a one-line phase/module marker with cumulative totals.
pub(super) fn print_marker(profiler: &Profiler, label: &str) {
    eprintln!(
        "  [alloc-profile] ▶ {} | total={} allocs={} | {} | {}",
        label,
        format_bytes(profiler.total_bytes),
        format_count(profiler.total_count),
        class_line(profiler),
        bump_fields(&current_arena_stats())
    );
}

/// The bump-arena tail every marker carries: `bump=off|on chunks=N
/// reserved=N used=N hw=N`. Keys are chosen not to contain any class key
/// (`mu=`, `env=`, `ref=`, `str=`, `other=`) as a substring, because the
/// `selfcompiled-profile` marker parser locates each class by `key=`
/// substring and would otherwise read an arena number as a class total.
pub(super) fn bump_fields(stats: &ArenaStatsOut) -> String {
    format!(
        "bump={} chunks={} reserved={} used={} hw={}",
        if stats.mode == MODE_BUMP { "on" } else { "off" },
        stats.chunks,
        format_bytes(stats.reserved),
        format_bytes(stats.used),
        format_bytes(stats.high_water)
    )
}

/// The report's closing line: what the arena did and what the profile does
/// not see — escape-analysis stack folds (ADR 8.5.26d) never reach the
/// runtime symbol, so they are absent from every count above.
pub(super) fn bump_summary(stats: &ArenaStatsOut) -> String {
    format!(
        "arena: {} (heap allocations through the runtime symbol only; stack folds excluded)",
        bump_fields(stats)
    )
}

/// Per-class summary table.
fn print_class_summary(profiler: &Profiler) {
    eprintln!("  {}", "─".repeat(78));
    eprintln!(
        "  {:24} {:>16} {:>8} {:>16}",
        "Class", "Bytes", "%", "Allocs"
    );
    eprintln!("  {}", "─".repeat(78));
    for (i, class_name) in CLASS_NAMES.iter().enumerate() {
        if profiler.class_count[i] == 0 {
            continue;
        }
        let pct = (profiler.class_bytes[i] as f64 / profiler.total_bytes as f64) * 100.0;
        eprintln!(
            "  {:24} {:>16} ({:>5.2}%) {:>16}",
            class_name,
            format_bytes(profiler.class_bytes[i]),
            pct,
            format_count(profiler.class_count[i])
        );
    }
}

/// mu_alloc size histogram — discriminates list spines / small nodes from
/// by-value payload copies (ADR 2.7.26a §2 axis (b), candidate 5).
fn print_mu_histogram(profiler: &Profiler) {
    let total: u64 = profiler.mu_size_hist.iter().sum();
    if total == 0 {
        return;
    }
    eprintln!("  {}", "─".repeat(78));
    eprintln!("  mu_alloc size histogram (allocs):");
    for (i, &count) in profiler.mu_size_hist.iter().enumerate() {
        let label = match MU_BUCKET_BOUNDS.get(i) {
            Some(bound) => format!("≤{}B", bound),
            None => format!(">{}B", MU_BUCKET_BOUNDS[MU_BUCKET_BOUNDS.len() - 1]),
        };
        if count == 0 {
            continue;
        }
        let pct = (count as f64 / total as f64) * 100.0;
        eprintln!(
            "    {:8} {:>16} ({:>5.2}%)",
            label,
            format_count(count),
            pct
        );
    }
}

/// One-line per-class byte totals, e.g. `mu=1.2GB env=300MB …`.
fn class_line(profiler: &Profiler) -> String {
    let short = ["mu", "env", "ref", "str", "other"];
    let mut parts = Vec::new();
    for (i, name) in short.iter().enumerate() {
        if profiler.class_bytes[i] > 0 {
            parts.push(format!(
                "{}={}",
                name,
                format_bytes(profiler.class_bytes[i])
            ));
        }
    }
    parts.join(" ")
}

/// Entry indices sorted by bytes descending.
fn sorted_indices(profiler: &Profiler) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..profiler.entry_count).collect();
    indices.sort_by(|&a, &b| profiler.entries[b].bytes.cmp(&profiler.entries[a].bytes));
    indices
}

/// Resolve the entry's function name for display.
fn entry_name(profiler: &Profiler, idx: usize) -> &str {
    let name = profiler.entries[idx].name;
    if name.is_null() {
        "<unknown>"
    } else {
        unsafe {
            core::ffi::CStr::from_ptr(name)
                .to_str()
                .unwrap_or("<invalid utf8>")
        }
    }
}

/// Resolve the report filter name (if set and non-empty).
fn resolve_filter(profiler: &Profiler) -> Option<&str> {
    if profiler.filter_fn.is_null() {
        return None;
    }
    let s = unsafe {
        core::ffi::CStr::from_ptr(profiler.filter_fn)
            .to_str()
            .unwrap_or("")
    };
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Short name of the class holding the largest share of an entry's bytes.
fn dominant_class(bytes_by_class: &[u64; NUM_CLASSES]) -> &'static str {
    let short = ["mu", "env", "ref", "str", "other"];
    let mut best = 0;
    for i in 1..NUM_CLASSES {
        if bytes_by_class[i] > bytes_by_class[best] {
            best = i;
        }
    }
    if bytes_by_class[best] == 0 {
        "-"
    } else {
        short[best]
    }
}

/// Format a byte count with comma separators.
pub(super) fn format_bytes(bytes: u64) -> String {
    let s = bytes.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a count with comma separators.
pub(super) fn format_count(count: u64) -> String {
    format_bytes(count) // Same formatting logic
}
