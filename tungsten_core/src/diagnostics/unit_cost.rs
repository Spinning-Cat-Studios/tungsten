//! Unit-cost census report logic (ADR 8.7.26a §2.1).
//!
//! Pure data-in/data-out half of `tungsten doctor check unit-cost`: threshold
//! parsing, ranking, rendering (table / JSON / serial-list), and the gate
//! verdict. The codegen-side glue that produces [`UnitCostRecord`]s lives in
//! `bootstrap/src/compile/unit_cost/`; keeping this half here puts it in the
//! coverage + mutation gate scope (see `diagnostics::mod` docs).

use std::fmt::Write as _;

/// Per-unit cost measurement captured around one codegen unit's compilation.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitCostRecord {
    /// Codegen unit name (`file__def`, ADR 9.5.26b).
    pub unit_name: String,
    /// Stable pre-assigned unit index — the emitted-filename prefix
    /// (`<index>_<unit>.ll`), also shown in the `[i/N]` progress tag.
    pub unit_index: usize,
    /// Wall time spent compiling the unit, in seconds.
    pub wall_time_secs: f64,
    /// Allocation volume attributed to the unit (bytes requested from the
    /// allocator on the compiling thread — NOT peak RSS; ADR 8.7.26a
    /// Decision 2).
    pub alloc_bytes: u64,
}

/// A `--threshold` bound: either wall time (`0.5s`) or allocation volume
/// (`8GB`). Memory is the load-bearing gate, time the cheaper proxy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CostThreshold {
    WallTimeSecs(f64),
    AllocBytes(u64),
}

const BYTES_PER_KIB: u64 = 1024;
const BYTES_PER_MIB: u64 = 1024 * 1024;
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

impl CostThreshold {
    /// Parse a threshold string: a time in seconds (`0.5s`, `30s`) or an
    /// allocation bound (`8GB`, `512MB`, `900KB`). Byte suffixes use binary
    /// multipliers (`GB` ≡ `GiB` = 2³⁰), matching the GiB figures the census
    /// reports.
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        let lower = trimmed.to_ascii_lowercase();
        let numeric_part = |suffix: &str| -> Result<f64, String> {
            let digits = &trimmed[..trimmed.len() - suffix.len()];
            digits.trim().parse::<f64>().map_err(|_| {
                format!("invalid threshold '{trimmed}': expected a number before '{suffix}'")
            })
        };
        let byte_bound = |suffix: &str, multiplier: u64| -> Result<Self, String> {
            let value = numeric_part(suffix)?;
            if value < 0.0 {
                return Err(format!(
                    "invalid threshold '{trimmed}': must be non-negative"
                ));
            }
            Ok(Self::AllocBytes((value * multiplier as f64) as u64))
        };
        // Longest suffix first so "gib" is not consumed as a trailing "b".
        if lower.ends_with("gib") {
            byte_bound("gib", BYTES_PER_GIB)
        } else if lower.ends_with("mib") {
            byte_bound("mib", BYTES_PER_MIB)
        } else if lower.ends_with("kib") {
            byte_bound("kib", BYTES_PER_KIB)
        } else if lower.ends_with("gb") {
            byte_bound("gb", BYTES_PER_GIB)
        } else if lower.ends_with("mb") {
            byte_bound("mb", BYTES_PER_MIB)
        } else if lower.ends_with("kb") {
            byte_bound("kb", BYTES_PER_KIB)
        } else if lower.ends_with('s') {
            let secs = numeric_part("s")?;
            if secs < 0.0 {
                return Err(format!(
                    "invalid threshold '{trimmed}': must be non-negative"
                ));
            }
            Ok(Self::WallTimeSecs(secs))
        } else {
            Err(format!(
                "invalid threshold '{trimmed}': expected a time like '0.5s' or a size like '8GB'"
            ))
        }
    }

    /// Whether `record` meets or exceeds this bound. Inclusive (`≥`) so the
    /// summary line, the serial list, and the gate all select the same set —
    /// matching the ADR 3.7.26b census convention ("units ≥ 0.5 s").
    #[must_use]
    pub fn is_met_by(&self, record: &UnitCostRecord) -> bool {
        match self {
            Self::WallTimeSecs(secs) => record.wall_time_secs >= *secs,
            Self::AllocBytes(bytes) => record.alloc_bytes >= *bytes,
        }
    }

    /// Human-readable form for summary lines (`0.5s`, `8.0GB`).
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::WallTimeSecs(secs) => format!("{secs}s"),
            Self::AllocBytes(bytes) => format_bytes(*bytes),
        }
    }
}

/// Format an allocation-volume figure for census lines: `6.4GB`, `512.0MB`,
/// `3.2KB`, `17B`. Binary multipliers (GB ≡ GiB), one decimal place.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= BYTES_PER_GIB {
        format!("{:.1}GB", bytes as f64 / BYTES_PER_GIB as f64)
    } else if bytes >= BYTES_PER_MIB {
        format!("{:.1}MB", bytes as f64 / BYTES_PER_MIB as f64)
    } else if bytes >= BYTES_PER_KIB {
        format!("{:.1}KB", bytes as f64 / BYTES_PER_KIB as f64)
    } else {
        format!("{bytes}B")
    }
}

/// Sort descending by the threshold's metric — allocation volume for an
/// alloc bound, wall time otherwise (also the no-threshold default, matching
/// the 3.7.26b census ranking). Ties break by unit name for determinism.
pub fn sort_ranked(records: &mut [UnitCostRecord], threshold: Option<&CostThreshold>) {
    let by_alloc = matches!(threshold, Some(CostThreshold::AllocBytes(_)));
    records.sort_by(|a, b| {
        let metric_order = if by_alloc {
            b.alloc_bytes.cmp(&a.alloc_bytes)
        } else {
            b.wall_time_secs
                .partial_cmp(&a.wall_time_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        metric_order.then_with(|| a.unit_name.cmp(&b.unit_name))
    });
}

/// The records meeting `threshold`, in the order given.
#[must_use]
pub fn units_meeting<'a>(
    records: &'a [UnitCostRecord],
    threshold: &CostThreshold,
) -> Vec<&'a UnitCostRecord> {
    records.iter().filter(|r| threshold.is_met_by(r)).collect()
}

/// Gate verdict: true when any unit meets the threshold (exit ≠ 0).
#[must_use]
pub fn gate_fails(records: &[UnitCostRecord], threshold: &CostThreshold) -> bool {
    records.iter().any(|r| threshold.is_met_by(r))
}

/// Render the ranked census table. With a threshold, rows are filtered to the
/// units meeting it and a summary line is appended:
/// `57 unit(s) ≥ 0.5s (of 2030), 760.0s of 786.0s total`.
/// Callers should [`sort_ranked`] first.
#[must_use]
pub fn render_table(records: &[UnitCostRecord], threshold: Option<&CostThreshold>) -> String {
    let name_width = records
        .iter()
        .map(|r| r.unit_name.len())
        .max()
        .unwrap_or(4)
        .max("unit".len());
    let mut out = format!("  {:<name_width$}  {:>8}  {:>9}\n", "unit", "time", "alloc");
    let shown: Vec<&UnitCostRecord> = match threshold {
        Some(t) => units_meeting(records, t),
        None => records.iter().collect(),
    };
    for record in &shown {
        let _ = writeln!(
            out,
            "  {:<name_width$}  {:>7.1}s  {:>9}",
            record.unit_name,
            record.wall_time_secs,
            format_bytes(record.alloc_bytes),
        );
    }
    if let Some(t) = threshold {
        // + 0.0 normalizes the -0.0 that f64's empty-iterator sum returns
        // (its additive identity is -0.0), which would print as "-0s".
        let shown_time: f64 = shown.iter().map(|r| r.wall_time_secs).sum::<f64>() + 0.0;
        let total_time: f64 = records.iter().map(|r| r.wall_time_secs).sum::<f64>() + 0.0;
        let _ = writeln!(
            out,
            "  {} unit(s) \u{2265} {} (of {}), {:.0}s of {:.0}s total",
            shown.len(),
            t.describe(),
            records.len(),
            shown_time,
            total_time,
        );
    }
    out
}

/// Render the machine-readable ranking. Callers should [`sort_ranked`] first.
#[must_use]
pub fn render_json(records: &[UnitCostRecord]) -> String {
    let units: Vec<String> = records
        .iter()
        .map(|r| {
            format!(
                "{{\"unit\":\"{}\",\"index\":{},\"time_s\":{:.3},\"alloc_bytes\":{}}}",
                escape_json(&r.unit_name),
                r.unit_index,
                r.wall_time_secs,
                r.alloc_bytes,
            )
        })
        .collect();
    // + 0.0: see render_table — empty f64 sums are -0.0.
    let total_time: f64 = records.iter().map(|r| r.wall_time_secs).sum::<f64>() + 0.0;
    let total_alloc: u64 = records.iter().map(|r| r.alloc_bytes).sum();
    format!(
        "{{\"units\":[{}],\"total_time_s\":{:.3},\"total_alloc_bytes\":{}}}",
        units.join(","),
        total_time,
        total_alloc,
    )
}

/// Render the comma-separated `TUNGSTEN_CODEGEN_SERIAL_UNITS` value for the
/// units meeting `threshold`, heaviest first (serial-queue drain order).
/// Callers should [`sort_ranked`] first.
#[must_use]
pub fn render_serial_list(records: &[UnitCostRecord], threshold: &CostThreshold) -> String {
    units_meeting(records, threshold)
        .iter()
        .map(|r| r.unit_name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Minimal JSON string escaping — unit names are `[A-Za-z0-9_]` in practice,
/// but quotes/backslashes/control bytes must never break the document.
fn escape_json(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(escaped, "\\u{:04x}", c as u32);
            }
            c => escaped.push(c),
        }
    }
    escaped
}

// Tests: unit_cost_tests.rs
#[cfg(test)]
#[path = "unit_cost_tests.rs"]
mod unit_cost_tests;
