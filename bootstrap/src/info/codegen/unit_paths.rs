//! `tungsten info codegen unit-paths` — where each codegen unit's `.ll` lands,
//! and which units would overwrite each other getting there.
//!
//! **Why this exists.** `tungsten compile --emit-llvm` on the self-hosted
//! compiler reports "✓ Wrote 2055 LLVM IR file(s)" and leaves 2049 files on
//! disk. Six units silently overwrote one another: the emitter counts *units*
//! while the filesystem holds *paths*, and macOS APFS is case-insensitive, so
//! `char_A.ll` and `char_a.ll` are the same file. Nothing reported it — the
//! discrepancy was found by hand, by re-emitting with `--verbose` (~95 s) and
//! post-processing the paths (ADR 28.7.26e §1.2 / retrospective).
//!
//! That matters beyond a confusing count. Every `doctor check ir` audit walks
//! the emitted tree, so an overwritten unit is a unit **no audit ever sees** —
//! a hole in the corpus gate that looks exactly like coverage.
//!
//! The destination rule is not restated here: it comes from
//! [`crate::compile::per_module::emit_paths`], the same function the emitter
//! uses, so this diagnostic cannot drift from what actually gets written.
//!
//! Cost 3 — elaborate only. The paths depend on the codegen *partitioning*,
//! which elaboration already computes; no LLVM is invoked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::driver;

use crate::compile::per_module::emit_paths::{self, OutsideSourceRoot, UnitOrigin};

#[cfg(test)]
mod tests;

/// One unit and the `.ll` path it will be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedUnit {
    /// The definition's source name (before the `main` → `tungsten_main` rename).
    pub def_name: String,
    /// The `.tg` file the definition came from.
    pub source_file: PathBuf,
    /// Where its `.ll` will be written.
    pub dest: PathBuf,
}

/// How badly a set of units sharing one destination collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionKind {
    /// Byte-for-byte the same path: these units overwrite each other on **every**
    /// filesystem. Always a defect.
    Exact,
    /// Paths that differ only in case: collapsed by a case-insensitive
    /// filesystem (APFS, NTFS), distinct on a case-sensitive one (ext4). So the
    /// emitted corpus differs by *host*, which is worse than a plain bug — the
    /// same compiler produces a different audit surface on macOS and Linux.
    CaseOnly,
}

impl CollisionKind {
    /// The one-line explanation printed with the group.
    pub fn human(self) -> &'static str {
        match self {
            CollisionKind::Exact => {
                "identical paths — these units overwrite each other on every filesystem"
            }
            CollisionKind::CaseOnly => {
                "paths differ only in case — collapsed on a case-insensitive filesystem \
                 (APFS, NTFS), distinct on ext4"
            }
        }
    }
}

/// Units that share one destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCollision {
    pub kind: CollisionKind,
    /// The destination they contend for (the first member's spelling).
    pub dest: PathBuf,
    pub units: Vec<PlacedUnit>,
}

/// Counts the command reports, and the basis of its exit status.
///
/// A tally rather than a bare `ExitCode` so the verdict is assertable: a test
/// that only compares exit codes asserts almost nothing, and
/// `ExitCode::default()` is SUCCESS — a gate that always passes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnitPathTally {
    /// Codegen units considered.
    pub units: usize,
    /// Distinct destinations after case-folding — what a case-insensitive
    /// filesystem will actually hold.
    pub distinct_paths: usize,
    pub exact_collisions: usize,
    pub case_collisions: usize,
    /// Units whose source file lies outside the source root, so no mirror path
    /// exists (the emitter hard-errors on these).
    pub unplaceable: usize,
}

impl UnitPathTally {
    /// Every unit gets its own file and every unit is placeable.
    pub fn is_clean(&self) -> bool {
        self.exact_collisions == 0 && self.case_collisions == 0 && self.unplaceable == 0
    }

    /// Non-zero on any collision or unplaceable unit, so CI can gate on it.
    pub fn exit(&self) -> ExitCode {
        if self.is_clean() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// The full plan: where every unit goes, plus what goes wrong.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnitPathPlan {
    pub placed: Vec<PlacedUnit>,
    pub unplaceable: Vec<OutsideSourceRoot>,
    pub collisions: Vec<PathCollision>,
}

impl UnitPathPlan {
    /// The counts, derived from the plan so they cannot disagree with it.
    pub fn tally(&self) -> UnitPathTally {
        UnitPathTally {
            units: self.placed.len() + self.unplaceable.len(),
            distinct_paths: self.distinct_paths(),
            exact_collisions: self.count_kind(CollisionKind::Exact),
            case_collisions: self.count_kind(CollisionKind::CaseOnly),
            unplaceable: self.unplaceable.len(),
        }
    }

    /// Destinations a case-insensitive filesystem would keep apart.
    fn distinct_paths(&self) -> usize {
        let folded: std::collections::BTreeSet<String> =
            self.placed.iter().map(|u| fold_path(&u.dest)).collect();
        folded.len()
    }

    fn count_kind(&self, kind: CollisionKind) -> usize {
        self.collisions.iter().filter(|c| c.kind == kind).count()
    }
}

/// Case-fold a path for collision keying — the identity a case-insensitive
/// filesystem uses.
fn fold_path(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Plan every unit's destination and group the ones that contend for a path.
///
/// Pure: plain paths in, plan out — no elaboration, no filesystem, no LLVM.
pub fn plan_unit_paths(
    origins: &[UnitOrigin<'_>],
    source_root: &Path,
    output_dir: &Path,
) -> UnitPathPlan {
    let mut plan = UnitPathPlan::default();
    // BTreeMap so the report order is deterministic (this is diagnostic output
    // people diff between runs).
    let mut by_folded_path: BTreeMap<String, Vec<PlacedUnit>> = BTreeMap::new();

    for origin in origins {
        match emit_paths::emit_llvm_dest(origin, source_root, output_dir) {
            Ok(dest) => {
                let placed = PlacedUnit {
                    def_name: origin.def_name.to_string(),
                    source_file: origin.source_file.to_path_buf(),
                    dest,
                };
                by_folded_path
                    .entry(fold_path(&placed.dest))
                    .or_default()
                    .push(placed.clone());
                plan.placed.push(placed);
            }
            Err(outside) => plan.unplaceable.push(outside),
        }
    }

    for units in by_folded_path.into_values() {
        if units.len() < 2 {
            continue;
        }
        plan.collisions.push(PathCollision {
            kind: classify(&units),
            dest: units[0].dest.clone(),
            units,
        });
    }
    plan
}

/// `Exact` when two members share a destination byte-for-byte, else `CaseOnly`.
///
/// Exact is the stronger finding, so a mixed group (`f`, `f`, `F`) reports as
/// exact: it is broken everywhere, not just on APFS.
fn classify(units: &[PlacedUnit]) -> CollisionKind {
    let distinct: std::collections::BTreeSet<&Path> =
        units.iter().map(|u| u.dest.as_path()).collect();
    if distinct.len() < units.len() {
        CollisionKind::Exact
    } else {
        CollisionKind::CaseOnly
    }
}

/// Show where each codegen unit's `.ll` will be written, and flag collisions.
pub fn cmd_info_codegen_unit_paths(
    file: &PathBuf,
    output: Option<&Path>,
    json: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let source_root = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let output_dir = crate::compile::per_module::emit_paths::resolve_emit_llvm_dir(file, output);
    let origins: Vec<UnitOrigin<'_>> = project
        .codegen_units
        .iter()
        .filter_map(|unit| {
            unit.defs.first().map(|def| UnitOrigin {
                source_file: unit.source_file.as_path(),
                def_name: def.name.as_str(),
            })
        })
        .collect();

    let plan = plan_unit_paths(&origins, &source_root, &output_dir);
    if json {
        print_json(&plan, &output_dir);
    } else {
        print_human(&plan, &output_dir);
    }
    plan.tally().exit()
}

fn print_human(plan: &UnitPathPlan, output_dir: &Path) {
    let tally = plan.tally();
    println!(
        "{} unit(s) → {} distinct path(s) under {}",
        tally.units,
        tally.distinct_paths,
        output_dir.display()
    );
    println!(
        "note: the synthetic mono depot adds {}, which cannot collide with a \
         per-function unit (those all sit in subdirectories).",
        emit_paths::mono_depot_dest(output_dir, crate::compile::mono::MONO_DEPOT_UNIT).display()
    );

    for outside in &plan.unplaceable {
        println!("\n✗ unplaceable: {outside}");
    }

    if plan.collisions.is_empty() {
        println!("\n✓ every unit has its own path");
        return;
    }
    println!(
        "\n⚠ {} colliding path(s) — a unit that loses a collision is written and \
         then overwritten, so NO `doctor check ir` audit ever sees it:",
        plan.collisions.len()
    );
    for collision in &plan.collisions {
        println!("\n  {}", collision.dest.display());
        println!("    {}", collision.kind.human());
        for unit in &collision.units {
            println!("    {} ← {}", unit.def_name, unit.source_file.display());
        }
    }
}

fn print_json(plan: &UnitPathPlan, output_dir: &Path) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json_value(plan, output_dir)).unwrap()
    );
}

/// The JSON body, as a value — separated from printing so a test can assert the
/// counts agree with the tally rather than merely that nothing panicked.
fn json_value(plan: &UnitPathPlan, output_dir: &Path) -> serde_json::Value {
    let tally = plan.tally();
    let collisions: Vec<_> = plan
        .collisions
        .iter()
        .map(|c| {
            serde_json::json!({
                "kind": match c.kind {
                    CollisionKind::Exact => "exact",
                    CollisionKind::CaseOnly => "case-only",
                },
                "dest": c.dest.display().to_string(),
                "units": c.units.iter().map(|u| serde_json::json!({
                    "def": u.def_name,
                    "source_file": u.source_file.display().to_string(),
                    "dest": u.dest.display().to_string(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "output_dir": output_dir.display().to_string(),
        "units": tally.units,
        "distinct_paths": tally.distinct_paths,
        "exact_collisions": tally.exact_collisions,
        "case_collisions": tally.case_collisions,
        "unplaceable": plan
            .unplaceable
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>(),
        "collisions": collisions,
    })
}
