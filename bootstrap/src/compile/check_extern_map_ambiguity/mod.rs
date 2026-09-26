//! `tungsten doctor check extern-map-ambiguity` — static detector for
//! colliding-name extern-map misresolution (ADR 12.7.26b).
//!
//! The per-unit `extern_name_map` is bare-name-keyed and clobber-last, so a
//! unit referencing a name defined in ≥ 2 modules silently calls whichever
//! candidate sorts last in `all_defs_info` key order (ADR 12.7.26a §1). This
//! check reuses the exact codegen-input pipeline (`gather_codegen_inputs`) and
//! the exact map-population/override functions real codegen uses (D2/D3), so
//! it can never disagree with what codegen would emit — but it stops before
//! LLVM emission (elaborate + partition only, the `check_mono_coverage`
//! pattern).
//!
//! Lives in `compile/` because everything it reuses is private to
//! `compile::per_module` (ADR 12.7.26b §2.1).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::driver::{self, ModuleCodegenUnit};

use super::mono;
use super::per_module::codegen_unit_name;
use super::per_module::compilation::extern_map::extend_extern_map_for_referenced;
use super::per_module::compilation::{nongeneric_referenced_globals, scoped_llvm_name};
use super::per_module::depot::referenced_globals_of_instance;
use super::per_module::imports::{resolve_collision, CollisionContext, CollisionResolution};
use super::per_module::inputs::{gather_codegen_inputs, CodegenInputs};
use super::{def_llvm_name, extern_wrap_name, CompileFlags};

#[cfg(test)]
mod tests;

/// One same-named def a colliding reference could resolve against (D4).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AmbiguityCandidate {
    /// `all_defs_info` composite key: `<owner_unit>::<def_name>`.
    pub(crate) def_key: String,
    /// The LLVM symbol this candidate defines.
    pub(crate) llvm_symbol: String,
    /// Whether the clobber-last map currently selects this candidate.
    pub(crate) clobber_winner: bool,
}

/// A `(unit, referenced name)` whose call target is silently ambiguous (D4).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AmbiguousReference {
    /// Codegen unit whose body makes the reference (`__mono` for depot).
    pub(crate) unit: String,
    /// The depot instance symbol, when the reference is inside a mono instance.
    pub(crate) mono_instance: Option<String>,
    /// The bare referenced name, as it appears in the term body.
    pub(crate) referenced_name: String,
    pub(crate) candidates: Vec<AmbiguityCandidate>,
}

/// Entry point for `tungsten doctor check extern-map-ambiguity <file>`.
pub fn cmd_check_extern_map_ambiguity(
    file: &PathBuf,
    json: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let units = &project.codegen_units;
    if units.is_empty() {
        println!("✓ No codegen units (single-module project). Nothing to check.");
        return ExitCode::SUCCESS;
    }

    let source_root = file.parent().unwrap_or(Path::new("."));
    let flags = CompileFlags {
        verbose,
        ..CompileFlags::default()
    };
    let inputs = match gather_codegen_inputs(units, source_root, &flags, &project) {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let findings = find_ambiguous_references(units, source_root, &inputs, &project);
    report_findings(&findings, units.len(), json)
}

/// Enumerate every ambiguous colliding-name reference across per-function
/// units and depot-owned mono instances.
///
/// A reference the ADR 12.7.26a provenance rule resolves (own-module def or
/// import-table target, via the same `resolve_collision` real codegen calls)
/// is not a finding; what remains is exactly what real codegen hard-errors on.
fn find_ambiguous_references(
    units: &[ModuleCodegenUnit],
    source_root: &Path,
    inputs: &CodegenInputs,
    project: &driver::ProjectOutput,
) -> Vec<AmbiguousReference> {
    let mut findings = Vec::new();
    let collision_ctx = CollisionContext {
        collision_index: &inputs.collision_index,
        all_defs_info: &inputs.all_defs_info,
        import_targets: &project.value_import_targets,
    };

    // Per-function units: model the exact map declare_unit_defs builds —
    // own-def entries first, then the referenced-extern population. The
    // ambiguity scan covers only references from bodies this unit actually
    // compiles: Forall-typed (generic) bodies are skipped by
    // `compile_unit_defs` and compile in the `__mono` depot instead, where
    // the depot pass below models them per instance (D3).
    for (unit, referenced) in units.iter().zip(&inputs.referenced_globals) {
        let unit_name = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
        let mut extern_map = own_def_map_entries(unit, &unit_name, inputs);
        extend_extern_map_for_referenced(
            referenced,
            &inputs.all_defs_info,
            &unit_name,
            &mut extern_map,
        );
        let compiled_body_refs = nongeneric_referenced_globals(unit);
        findings.extend(collect_unit_ambiguities(
            UnitScan {
                unit: &unit_name,
                mono_instance: None,
                own_module_path: &unit.module_path,
            },
            &compiled_body_refs,
            &extern_map,
            &collision_ctx,
        ));
    }

    // Depot instances: shared map over the union of instance refs, then the
    // per-instance provenance resolution (ADR 12.7.26a D5).
    let depot_owned = inputs.mono_map.owned_by(&mono::CodegenUnitId::mono_depot());
    let instance_refs: Vec<BTreeSet<String>> = depot_owned
        .iter()
        .map(|o| referenced_globals_of_instance(&inputs.poly_term_registry, &o.key.def_id.name))
        .collect();
    let all_referenced: BTreeSet<String> = instance_refs.iter().flatten().cloned().collect();
    let mut depot_map = HashMap::new();
    extend_extern_map_for_referenced(
        &all_referenced,
        &inputs.all_defs_info,
        mono::MONO_DEPOT_UNIT,
        &mut depot_map,
    );
    for (ownership, refs) in depot_owned.iter().zip(&instance_refs) {
        findings.extend(collect_unit_ambiguities(
            UnitScan {
                unit: mono::MONO_DEPOT_UNIT,
                mono_instance: Some(&ownership.symbol),
                own_module_path: &ownership.key.def_id.module_path,
            },
            refs,
            &depot_map,
            &collision_ctx,
        ));
    }

    findings
}

/// One scan target: a per-function unit, or one mono instance inside the depot.
struct UnitScan<'a> {
    unit: &'a str,
    mono_instance: Option<&'a str>,
    /// Canonical module path resolution runs against (own-module rule).
    own_module_path: &'a [String],
}

/// The `original → scoped` entries `declare_unit_defs` records for a unit's
/// own defs, before the referenced-extern population runs.
fn own_def_map_entries(
    unit: &ModuleCodegenUnit,
    unit_name: &str,
    inputs: &CodegenInputs,
) -> HashMap<String, String> {
    let mut extern_map = HashMap::new();
    for def in &unit.defs {
        let original = def_llvm_name(&def.name);
        let llvm_name = if let Some((orig, wrap)) = extern_wrap_name(&def.name, &def.term) {
            extern_map.insert(orig, wrap.clone());
            wrap
        } else {
            scoped_llvm_name(&def.name, unit_name, &inputs.collisions)
        };
        if llvm_name != original {
            extern_map.insert(original, llvm_name);
        }
    }
    extern_map
}

/// Report each referenced colliding name the ADR 12.7.26a provenance rule
/// cannot resolve, naming every candidate and the map's current pick (D4).
fn collect_unit_ambiguities(
    scan: UnitScan<'_>,
    referenced: &BTreeSet<String>,
    extern_map: &HashMap<String, String>,
    collision_ctx: &CollisionContext<'_>,
) -> Vec<AmbiguousReference> {
    let mut findings = Vec::new();
    for name in referenced {
        let candidate_keys: Vec<String> =
            match resolve_collision(name, scan.own_module_path, collision_ctx) {
                // Unique, or resolved by the own-module → import-table rule
                // (D5) — real codegen compiles this reference correctly.
                CollisionResolution::NotColliding | CollisionResolution::Resolved(_) => continue,
                CollisionResolution::Unresolvable { candidates } => candidates,
            };
        let original_llvm = def_llvm_name(name);
        let winner = extern_map.get(&original_llvm);
        let candidates: Vec<AmbiguityCandidate> = candidate_keys
            .into_iter()
            .map(|def_key| {
                let llvm_symbol = collision_ctx
                    .all_defs_info
                    .get(&def_key)
                    .map(|info| info.llvm_name.clone())
                    .unwrap_or_default();
                AmbiguityCandidate {
                    clobber_winner: Some(&llvm_symbol) == winner,
                    def_key,
                    llvm_symbol,
                }
            })
            .collect();
        findings.push(AmbiguousReference {
            unit: scan.unit.to_string(),
            mono_instance: scan.mono_instance.map(str::to_string),
            referenced_name: name.clone(),
            candidates,
        });
    }
    findings
}

/// Print findings (human or `--json`) and derive the gating exit code (D1).
fn report_findings(findings: &[AmbiguousReference], unit_count: usize, json: bool) -> ExitCode {
    if json {
        match serde_json::to_string_pretty(&findings) {
            Ok(rendered) => println!("{rendered}"),
            Err(e) => {
                eprintln!("error: JSON serialization failed: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print!("{}", render_human(findings, unit_count));
    }
    if findings.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Render the human report: one block per finding naming unit, referenced
/// name, every candidate, and the clobber-last winner (D4).
fn render_human(findings: &[AmbiguousReference], unit_count: usize) -> String {
    if findings.is_empty() {
        return format!(
            "✓ No ambiguous colliding-name references across {} codegen unit(s).\n",
            unit_count
        );
    }
    let mut report = format!("✗ {} ambiguous extern-map reference(s):\n", findings.len());
    for finding in findings {
        let location = match &finding.mono_instance {
            Some(instance) => format!("{}[{}]", finding.unit, instance),
            None => finding.unit.clone(),
        };
        report.push_str(&format!(
            "\n  {} → `{}`\n",
            location, finding.referenced_name
        ));
        let candidates: &[AmbiguityCandidate] = &finding.candidates;
        for candidate in candidates {
            let marker = if candidate.clobber_winner {
                "   ← clobber-last picks this"
            } else {
                ""
            };
            report.push_str(&format!(
                "    {}  ({}){}\n",
                candidate.def_key, candidate.llvm_symbol, marker
            ));
        }
    }
    report
}
