//! Parse a profiled self-compiled check's stderr stream into a per-module summary.
//!
//! Input lines (produced by an `--alloc-profile` tungsten1, ADR 2.7.26a):
//!
//! ```text
//!   [alloc-profile] ▶ phaseB:module elab::types | total=2,847,027,626 allocs=39,314,149 | mu=... env=... str=...
//!   [arena] types=3574163 deep=8990MB terms=57524 deep=12166MB ctxs=0 slab=298MB vmrss=29230MB
//! ```
//!
//! Markers fire at module *start*, so a delta between consecutive snapshots
//! belongs to the module named in the EARLIER marker (see
//! `tg_alloc_profile_marker`'s doc in `tungsten_core`).
//!
//! The marker's trailing `mu=/env=/ref=/str=/other=` fields are the per-class
//! allocation split (emitted by `class_line` in
//! `tungsten_runtime/src/alloc_profile/report.rs`, in that order, each field
//! omitted when its class total is zero). The parser keeps them so the
//! `--by-class` and `--growth` renderers can attribute the RSS ramp to a class
//! and diagnose its growth order — the two hand-parsed questions ADR 23.7.26d
//! answered in an ad-hoc Python script, now productized (ADR 24.7.26a).
//!
//! Since ADR 14.9.26b a marker line ends with one more `|`-separated segment,
//! the bump-arena tail `bump=off|on chunks=N reserved=N used=N hw=N`. Each
//! class is located by `key=` substring, and none of those keys contains a
//! class key, so the tail parses through this file unchanged — a regression
//! test below pins that.

pub mod render;
pub use render::render_summary;

/// Allocation classes carried on each marker line, in emitter field order
/// (`class_line`'s `["mu", "env", "ref", "str", "other"]`).
pub const NUM_CLASSES: usize = 5;
pub const CLASS_SHORT: [&str; NUM_CLASSES] = ["mu", "env", "ref", "str", "other"];

/// The `key=` token each class occupies on a marker line, e.g. `mu=`.
const CLASS_KEYS: [&str; NUM_CLASSES] = ["mu=", "env=", "ref=", "str=", "other="];

/// Marker labels for per-module phase-B snapshots carry this prefix.
pub(crate) const MODULE_MARKER_PREFIX: &str = "phaseB:module ";

/// One marker line plus its trailing `[arena]` retention line (if any).
pub struct MarkerSnapshot {
    pub label: String,
    /// Cumulative bytes from the marker's `total=` field (self-host-side classes).
    pub alloc_total_bytes: u64,
    /// Cumulative bytes per allocation class (`CLASS_SHORT` order). A class
    /// absent from the line (its total was zero) parses as 0.
    pub by_class: [u64; NUM_CLASSES],
    pub arena: Option<ArenaSnapshot>,
}

/// Parsed `[arena]` line: cumulative deep MB per class + VmRSS anchor.
pub struct ArenaSnapshot {
    pub types_deep_mb: u64,
    pub terms_deep_mb: u64,
    pub slab_mb: u64,
    pub vmrss_mb: u64,
}

/// Extract marker/arena snapshots from a profiled run's stderr text.
pub fn parse_profile_log(text: &str) -> Vec<MarkerSnapshot> {
    let mut snapshots: Vec<MarkerSnapshot> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("[alloc-profile] ▶ ") {
            let label = rest.split(" | ").next().unwrap_or("").trim().to_string();
            snapshots.push(MarkerSnapshot {
                label,
                alloc_total_bytes: field_u64(rest, "total="),
                by_class: parse_class_fields(rest),
                arena: None,
            });
        } else if let Some(rest) = line.trim_start().strip_prefix("[arena] ") {
            if let Some(last) = snapshots.last_mut() {
                if last.arena.is_none() {
                    last.arena = Some(parse_arena_line(rest));
                }
            }
        }
    }
    snapshots
}

/// Read the five per-class byte totals from a marker's `rest`, using the same
/// single-key extractor as `total=`. Absent classes read as 0.
fn parse_class_fields(rest: &str) -> [u64; NUM_CLASSES] {
    let mut by_class = [0u64; NUM_CLASSES];
    for (slot, key) in by_class.iter_mut().zip(CLASS_KEYS) {
        *slot = field_u64(rest, key);
    }
    by_class
}

/// Parse `types=N deep=NMB terms=N deep=NMB ctxs=N slab=NMB vmrss=NMB`.
/// The first `deep=` follows `types=`, the second follows `terms=`.
fn parse_arena_line(rest: &str) -> ArenaSnapshot {
    let mut deep = [0u64; 2];
    let mut deep_seen = 0;
    let mut slab_mb = 0;
    let mut vmrss_mb = 0;
    for token in rest.split_whitespace() {
        if let Some(v) = token.strip_prefix("deep=") {
            if deep_seen < 2 {
                deep[deep_seen] = parse_number(v);
                deep_seen += 1;
            }
        } else if let Some(v) = token.strip_prefix("slab=") {
            slab_mb = parse_number(v);
        } else if let Some(v) = token.strip_prefix("vmrss=") {
            vmrss_mb = parse_number(v);
        }
    }
    ArenaSnapshot {
        types_deep_mb: deep[0],
        terms_deep_mb: deep[1],
        slab_mb,
        vmrss_mb,
    }
}

/// Read the u64 following `key` (e.g. `total=`), tolerating thousands commas.
fn field_u64(text: &str, key: &str) -> u64 {
    text.split(key)
        .nth(1)
        .map(|rest| {
            parse_number(
                rest.split(|c: char| c.is_whitespace() || c == '|')
                    .next()
                    .unwrap_or(""),
            )
        })
        .unwrap_or(0)
}

/// Parse a number that may carry thousands commas and/or an `MB` suffix.
/// Non-numeric values (e.g. `vmrss=na` off-Linux) parse as 0.
fn parse_number(token: &str) -> u64 {
    let cleaned: String = token.chars().filter(char::is_ascii_digit).collect();
    cleaned.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
  [alloc-profile] ▶ phaseB:start | total=128,934,553 allocs=2,540,113 | mu=122,002,720 env=4,754,412 str=2,177,421
  [arena] types=14888 deep=146MB terms=0 deep=0MB ctxs=0 slab=1MB vmrss=372MB
  [alloc-profile] ▶ phaseB:module lexer::scanner | total=200,000,000 allocs=3,000,000 | mu=190,000,000 env=5,000,000 str=2,200,000
  [arena] types=20000 deep=200MB terms=100 deep=50MB ctxs=0 slab=2MB vmrss=500MB
  [alloc-profile] ▶ phaseB:module parser::exprs | total=400,000,000 allocs=5,000,000 | mu=380,000,000 env=6,000,000 ref=1,000,000 str=2,300,000 other=500,000
  [arena] types=30000 deep=300MB terms=2000 deep=2250MB ctxs=0 slab=3MB vmrss=3000MB
";

    #[test]
    fn parses_markers_and_arena_lines() {
        let snaps = parse_profile_log(SAMPLE);
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].label, "phaseB:start");
        assert_eq!(snaps[0].alloc_total_bytes, 128_934_553);
        let arena = snaps[2].arena.as_ref().unwrap();
        assert_eq!(arena.types_deep_mb, 300);
        assert_eq!(arena.terms_deep_mb, 2250);
        assert_eq!(arena.slab_mb, 3);
        assert_eq!(arena.vmrss_mb, 3000);
    }

    #[test]
    fn retains_all_five_class_fields() {
        let snaps = parse_profile_log(SAMPLE);
        // phaseB:start carries only mu/env/str; ref/other absent → 0.
        assert_eq!(snaps[0].by_class, [122_002_720, 4_754_412, 0, 2_177_421, 0]);
        // parser::exprs carries all five in emitter order.
        assert_eq!(
            snaps[2].by_class,
            [380_000_000, 6_000_000, 1_000_000, 2_300_000, 500_000]
        );
    }

    #[test]
    fn class_index_order_matches_class_short() {
        // Guard the CLASS_SHORT / CLASS_KEYS / by_class index alignment: a
        // renderer that indexes by_class[i] must get CLASS_SHORT[i].
        assert_eq!(CLASS_SHORT.len(), NUM_CLASSES);
        assert_eq!(CLASS_KEYS.len(), NUM_CLASSES);
        for (short, key) in CLASS_SHORT.iter().zip(CLASS_KEYS) {
            assert_eq!(key, &format!("{short}="));
        }
    }

    #[test]
    fn parse_arena_captures_only_the_first_two_deep_tokens() {
        // types-deep then terms-deep; a spurious third `deep=` must be ignored
        // (the `deep_seen < 2` bound), not indexed into the length-2 array.
        let text = "\
  [alloc-profile] ▶ phaseB:module a | total=1 allocs=1 | mu=1
  [arena] types=1 deep=10MB terms=1 deep=20MB ctxs=0 deep=99MB slab=3MB vmrss=40MB
";
        let snaps = parse_profile_log(text);
        let arena = snaps[0].arena.as_ref().unwrap();
        assert_eq!(arena.types_deep_mb, 10);
        assert_eq!(arena.terms_deep_mb, 20);
        assert_eq!(arena.slab_mb, 3);
        assert_eq!(arena.vmrss_mb, 40);
    }

    /// ADR 14.9.26b AC 8: a marker carrying the bump-arena tail still yields
    /// the class split and total, and no arena number is read as a class.
    /// `reserved=`/`used=`/`hw=`/`chunks=`/`bump=` share no `key=` substring
    /// with `mu=`/`env=`/`ref=`/`str=`/`other=`; the values are chosen so a
    /// wrong pick would be visible (each arena number is distinct from every
    /// class number).
    #[test]
    fn class_split_survives_the_bump_arena_tail() {
        let text = "\
  [alloc-profile] ▶ phaseB:module a | total=1,000 allocs=10 | mu=700 env=200 str=100 | bump=on chunks=3 reserved=12,582,912 used=999,999 hw=999,999
  [alloc-profile] ▶ phaseB:module b | total=2,000 allocs=20 | mu=1,400 env=300 ref=50 str=200 other=50 | bump=off chunks=0 reserved=0 used=0 hw=0
";
        let snaps = parse_profile_log(text);
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].alloc_total_bytes, 1_000);
        assert_eq!(snaps[0].by_class, [700, 200, 0, 100, 0]);
        assert_eq!(snaps[1].alloc_total_bytes, 2_000);
        assert_eq!(snaps[1].by_class, [1_400, 300, 50, 200, 50]);
        for key in ["bump=", "chunks=", "reserved=", "used=", "hw="] {
            for class_key in CLASS_KEYS {
                assert!(
                    !key.contains(class_key),
                    "{key} would be read as {class_key}"
                );
            }
        }
    }

    #[test]
    fn tolerates_missing_arena_lines_and_na_rss() {
        let text = "\
  [alloc-profile] ▶ phaseB:module a | total=1,000,000 allocs=10 | mu=1 env=1 str=1
  [alloc-profile] ▶ phaseB:module b | total=3,000,000 allocs=20 | mu=2 env=1 str=1
  [arena] types=1 deep=0MB terms=0 deep=0MB ctxs=0 slab=0MB vmrss=naMB
";
        let snaps = parse_profile_log(text);
        assert_eq!(snaps.len(), 2);
        assert!(snaps[0].arena.is_none());
        assert_eq!(snaps[1].arena.as_ref().unwrap().vmrss_mb, 0);
    }
}
