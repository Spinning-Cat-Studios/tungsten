//! Codegen unit selection and scheduling (ADR 3.7.26b).
//!
//! Two features key off codegen unit names here:
//!
//! - `--only-unit` isolation: compile just the named unit(s) for memory/time
//!   profiling, keeping cross-module declares and mono ownership from the
//!   full unit set ([`select_units_by_name`]).
//! - Serial-unit scheduling (Stub Registration mitigation): units named in
//!   `TUNGSTEN_CODEGEN_SERIAL_UNITS` are drained one-at-a-time by a single
//!   dedicated worker so pathological multi-GB units never compile
//!   concurrently ([`partition_serial_units`]).
//!
//! [`UnitWork`] pairs each unit with its derived name and referenced
//! globals so downstream consumers never rely on positional alignment
//! between parallel arrays.

use std::collections::BTreeSet;
use std::path::Path;

use tungsten_bootstrap::driver::ModuleCodegenUnit;

use super::codegen_unit_name;
use crate::compile::CompileFlags;

/// A codegen unit paired with its derived name and pre-computed referenced
/// globals. Pairing replaces the positional-parallel arrays (`units[i]` /
/// `referenced_globals[i]` / per-site name recomputation) whose index
/// alignment was an implicit invariant every consumer preserved by hand.
pub(super) struct UnitWork<'a> {
    pub(super) unit: &'a ModuleCodegenUnit,
    /// Deterministic unit name (ADR 9.5.26b), computed once at pairing.
    pub(super) unit_name: String,
    /// Cross-unit globals this unit references (ADR 10.5.26h §2.3).
    pub(super) referenced_globals: &'a BTreeSet<String>,
}

/// Pair each unit with its name and referenced globals — the single place
/// where positional alignment between the two source arrays is assumed.
pub(super) fn pair_units_with_globals<'a>(
    units: &'a [ModuleCodegenUnit],
    source_root: &Path,
    referenced_globals: &'a [BTreeSet<String>],
) -> Vec<UnitWork<'a>> {
    debug_assert_eq!(units.len(), referenced_globals.len());
    units
        .iter()
        .zip(referenced_globals)
        .map(|(unit, globals)| UnitWork {
            unit_name: codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name),
            unit,
            referenced_globals: globals,
        })
        .collect()
}

/// Unit compilation plan (ADR 3.7.26b): the units to compile plus the
/// worker schedule for the parallel driver.
pub(super) struct UnitPlan<'a> {
    /// Units to compile, in original unit order (an `--only-unit` subset
    /// when isolation is active).
    pub(super) work: Vec<UnitWork<'a>>,
    /// Worker schedule; indices index into `work`.
    pub(super) schedule: UnitSchedule,
    /// False under `--only-unit` isolation — the `__mono` depot aggregates
    /// specializations from the whole program, which defeats isolation.
    pub(super) compile_depot: bool,
}

/// Resolve `--only-unit` isolation and `TUNGSTEN_CODEGEN_SERIAL_UNITS`
/// scheduling for this compile. Isolation disables serial scheduling — a
/// profiling run wants the unit's raw behavior. A stale serial name warns
/// loudly: it means the OOM mitigation is no longer protecting the build
/// (ADR 3.7.26b §2.1).
pub(super) fn plan_unit_schedule<'u>(
    work: Vec<UnitWork<'u>>,
    flags: &CompileFlags,
) -> Result<UnitPlan<'u>, String> {
    if !flags.diagnostics.only_units.is_empty() {
        let unit_names: Vec<&str> = work.iter().map(|w| w.unit_name.as_str()).collect();
        let indices = select_units_by_name(&unit_names, &flags.diagnostics.only_units)?;
        if flags.verbose {
            eprintln!(
                "[only-unit] compiling {} of {} unit(s); __mono depot skipped",
                indices.len(),
                work.len()
            );
        }
        let selected: Vec<UnitWork<'_>> = work
            .into_iter()
            .enumerate()
            .filter(|(unit_idx, _)| indices.binary_search(unit_idx).is_ok())
            .map(|(_, item)| item)
            .collect();
        return Ok(UnitPlan {
            schedule: UnitSchedule::all_parallel(selected.len()),
            work: selected,
            compile_depot: false,
        });
    }

    let unit_names: Vec<&str> = work.iter().map(|w| w.unit_name.as_str()).collect();
    let (schedule, unmatched) = partition_serial_units(&unit_names, &flags.codegen_serial_units);
    if !unmatched.is_empty() {
        eprintln!(
            "warning: TUNGSTEN_CODEGEN_SERIAL_UNITS names {} unknown unit(s): {} — \
             the OOM mitigation list may be stale (ADR 3.7.26b)",
            unmatched.len(),
            unmatched.join(", ")
        );
    }
    if flags.verbose && !schedule.serial.is_empty() {
        eprintln!(
            "[serial-units] {} unit(s) serialized onto one worker",
            schedule.serial.len()
        );
    }
    Ok(UnitPlan {
        work,
        schedule,
        compile_depot: true,
    })
}

/// How the parallel codegen driver distributes unit indices across workers
/// (ADR 3.7.26b Stub Registration).
pub(super) struct UnitSchedule {
    /// Indices compiled one-at-a-time on a single dedicated worker,
    /// in order. Empty for unmitigated builds.
    pub(super) serial: Vec<usize>,
    /// Indices compiled by the work-stealing worker pool.
    pub(super) parallel: Vec<usize>,
}

impl UnitSchedule {
    /// Schedule with no serialized units: everything work-steals.
    pub(super) fn all_parallel(unit_count: usize) -> Self {
        UnitSchedule {
            serial: Vec::new(),
            parallel: (0..unit_count).collect(),
        }
    }
}

/// Resolve `--only-unit` names to indices into `unit_names`, preserving
/// unit order and deduplicating. An unknown name is an error naming the
/// closest existing units, so a typo can't silently compile nothing.
pub(super) fn select_units_by_name(
    unit_names: &[impl AsRef<str>],
    requested: &[String],
) -> Result<Vec<usize>, String> {
    let mut selected: Vec<usize> = Vec::new();
    for name in requested {
        let matches: Vec<usize> = unit_names
            .iter()
            .enumerate()
            .filter(|(_, unit_name)| unit_name.as_ref() == name)
            .map(|(idx, _)| idx)
            .collect();
        if matches.is_empty() {
            return Err(unknown_unit_error(name, unit_names));
        }
        selected.extend(matches);
    }
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

/// Split unit indices into (schedule, unmatched-serial-names). Serial units
/// keep their original relative order; names that match no unit are returned
/// so the caller can warn — a stale mitigation list is an OOM risk, not a
/// silent no-op (ADR 3.7.26b §2.1).
pub(super) fn partition_serial_units(
    unit_names: &[impl AsRef<str>],
    serial_names: &[String],
) -> (UnitSchedule, Vec<String>) {
    if serial_names.is_empty() {
        return (UnitSchedule::all_parallel(unit_names.len()), Vec::new());
    }
    let mut serial = Vec::new();
    let mut parallel = Vec::new();
    for (idx, unit_name) in unit_names.iter().enumerate() {
        if serial_names.iter().any(|name| name == unit_name.as_ref()) {
            serial.push(idx);
        } else {
            parallel.push(idx);
        }
    }
    let unmatched: Vec<String> = serial_names
        .iter()
        .filter(|name| !unit_names.iter().any(|n| n.as_ref() == name.as_str()))
        .cloned()
        .collect();
    (UnitSchedule { serial, parallel }, unmatched)
}

/// Error text for an `--only-unit` name that matches no codegen unit,
/// suggesting the closest existing names by edit distance.
fn unknown_unit_error(requested: &str, unit_names: &[impl AsRef<str>]) -> String {
    let mut ranked: Vec<(usize, &str)> = unit_names
        .iter()
        .map(|name| {
            (
                tungsten_bootstrap::utils::levenshtein_distance(requested, name.as_ref()),
                name.as_ref(),
            )
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    let suggestions: Vec<String> = ranked
        .iter()
        .take(3)
        .map(|(_, name)| format!("  {}", name))
        .collect();
    format!(
        "--only-unit '{}' matches no codegen unit. Closest units:\n{}\n\
         (list all with: tungsten info codegen units <file>)",
        requested,
        suggestions.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn names(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    /// Minimal single-def unit for pairing tests (mirrors tests/globals.rs).
    fn make_unit(source_file: &str, def_name: &str) -> ModuleCodegenUnit {
        use tungsten_bootstrap::elaborate::CoreDef;
        use tungsten_bootstrap::Span;
        use tungsten_core::terms::{SpannedTerm, Term};
        use tungsten_core::types::Type;

        ModuleCodegenUnit {
            module_path: vec![],
            source_file: PathBuf::from(source_file),
            defs: vec![CoreDef {
                name: def_name.to_string(),
                ty: Type::Nat,
                term: SpannedTerm {
                    term: Term::Zero,
                    span: None,
                },
                span: Span::new(0, 0),
            }],
        }
    }

    fn flags_with(only_units: Vec<String>, serial_units: Vec<String>) -> CompileFlags {
        let mut flags = CompileFlags::default();
        flags.diagnostics.only_units = only_units;
        flags.codegen_serial_units = serial_units;
        flags
    }

    #[test]
    fn pairing_derives_names_and_keeps_globals_aligned() {
        let units = vec![
            make_unit("/src/parser/pratt.tg", "parse_unary_op"),
            make_unit("/src/lexer.tg", "scan_token"),
        ];
        let globals = vec![
            BTreeSet::from(["g_a".to_string()]),
            BTreeSet::from(["g_b".to_string()]),
        ];
        let work = pair_units_with_globals(&units, Path::new("/src"), &globals);
        assert_eq!(work.len(), 2);
        assert_eq!(work[0].unit_name, "parser__pratt__parse_unary_op");
        assert!(work[0].referenced_globals.contains("g_a"));
        assert_eq!(work[1].unit_name, "lexer__scan_token");
        assert!(work[1].referenced_globals.contains("g_b"));
    }

    #[test]
    fn plan_isolation_selects_pairs_and_skips_depot() {
        let units = vec![make_unit("/src/a.tg", "f"), make_unit("/src/b.tg", "g")];
        let globals = vec![BTreeSet::new(), BTreeSet::from(["g_b".to_string()])];
        let work = pair_units_with_globals(&units, Path::new("/src"), &globals);
        let flags = flags_with(vec!["b__g".to_string()], vec![]);
        let plan = plan_unit_schedule(work, &flags).unwrap();
        assert_eq!(plan.work.len(), 1);
        assert_eq!(plan.work[0].unit_name, "b__g");
        assert!(plan.work[0].referenced_globals.contains("g_b"));
        assert!(!plan.compile_depot);
        assert!(plan.schedule.serial.is_empty());
        assert_eq!(plan.schedule.parallel, vec![0]);
    }

    #[test]
    fn plan_serial_units_partition_and_keep_depot() {
        let units = vec![
            make_unit("/src/a.tg", "f"),
            make_unit("/src/heavy.tg", "explode"),
        ];
        let globals = vec![BTreeSet::new(), BTreeSet::new()];
        let work = pair_units_with_globals(&units, Path::new("/src"), &globals);
        let flags = flags_with(vec![], vec!["heavy__explode".to_string()]);
        let plan = plan_unit_schedule(work, &flags).unwrap();
        assert_eq!(plan.work.len(), 2);
        assert!(plan.compile_depot);
        assert_eq!(plan.schedule.serial, vec![1]);
        assert_eq!(plan.schedule.parallel, vec![0]);
    }

    #[test]
    fn select_finds_units_in_original_order_and_dedups() {
        let unit_names = names(&["a__f", "b__g", "c__h"]);
        let requested = names(&["c__h", "a__f", "c__h"]);
        let selected = select_units_by_name(&unit_names, &requested).unwrap();
        assert_eq!(selected, vec![0, 2]);
    }

    #[test]
    fn select_unknown_name_errors_with_closest_suggestions() {
        let unit_names = names(&["parser__pratt__parse_unary_op", "lexer__scan_token"]);
        let err = select_units_by_name(&unit_names, &names(&["parser__pratt__parse_unary_pp"]))
            .unwrap_err();
        assert!(err.contains("matches no codegen unit"), "{err}");
        assert!(err.contains("parser__pratt__parse_unary_op"), "{err}");
        assert!(err.contains("info codegen units"), "{err}");
    }

    #[test]
    fn partition_empty_serial_list_is_all_parallel() {
        let unit_names = names(&["a__f", "b__g"]);
        let (schedule, unmatched) = partition_serial_units(&unit_names, &[]);
        assert!(schedule.serial.is_empty());
        assert_eq!(schedule.parallel, vec![0, 1]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn partition_splits_serial_from_parallel_preserving_order() {
        let unit_names = names(&["a__f", "heavy__1", "b__g", "heavy__2"]);
        let (schedule, unmatched) =
            partition_serial_units(&unit_names, &names(&["heavy__2", "heavy__1"]));
        assert_eq!(schedule.serial, vec![1, 3]);
        assert_eq!(schedule.parallel, vec![0, 2]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn partition_reports_stale_serial_names() {
        let unit_names = names(&["a__f"]);
        let (schedule, unmatched) =
            partition_serial_units(&unit_names, &names(&["renamed__away", "a__f"]));
        assert_eq!(schedule.serial, vec![0]);
        assert!(schedule.parallel.is_empty());
        assert_eq!(unmatched, names(&["renamed__away"]));
    }

    #[test]
    fn all_parallel_covers_every_index() {
        let schedule = UnitSchedule::all_parallel(3);
        assert!(schedule.serial.is_empty());
        assert_eq!(schedule.parallel, vec![0, 1, 2]);
    }
}
