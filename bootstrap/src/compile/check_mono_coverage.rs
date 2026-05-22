//! `tungsten doctor check-mono-coverage` — detect uncovered TyApp sites.
//!
//! Walks all codegen unit term trees and verifies every
//! `TyApp(Global(name), ty_arg)` has a corresponding entry in the frozen
//! mono ownership map. Reports uncovered sites that would ICE during codegen.
//!
//! Lives in `compile/` because it imports from `compile::mono` which
//! is a binary-side module (ADR 8.5.26i §2.5).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_core::terms::Term;

use tungsten_bootstrap::driver::{self, ModuleCodegenUnit};

use super::mono::{
    assign_owners, discover_mono_requests, CanonicalTypeArgs, DefId, MonoKey, MonoOwnershipMap,
};
use super::per_module::codegen_unit_name;

/// An uncovered `TyApp(Global(name), ty)` site.
#[derive(Debug)]
struct UncoveredSite {
    def_name: String,
    type_arg_display: String,
    unit_id: String,
}

/// Entry point for `tungsten doctor check-mono-coverage <file>`.
pub fn cmd_check_mono_coverage(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
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

    let concrete_type_names = project.concrete_type_names();

    // Run the full mono pipeline (same path as codegen)
    let mut table = discover_mono_requests(units, source_root, &concrete_type_names);
    let unit_names: Vec<String> = units
        .iter()
        .map(|u| codegen_unit_name(&u.source_file, source_root, &u.defs[0].name))
        .collect();
    table.freeze();
    let mono_map = assign_owners(&table, &unit_names);

    // Walk all terms again and check coverage
    let def_map = build_def_map(units);
    let uncovered = find_uncovered_sites(units, source_root, &def_map, &mono_map);
    let total_sites = count_tyapp_sites(units);

    if uncovered.is_empty() {
        println!(
            "✓ All {} TyApp(Global(...), ...) site(s) covered by mono ownership map.",
            total_sites
        );
        ExitCode::SUCCESS
    } else {
        println!("✗ {} uncovered TyApp site(s):", uncovered.len());
        for site in &uncovered {
            println!(
                "  - {}<{}>  (in {})",
                site.def_name, site.type_arg_display, site.unit_id
            );
        }
        ExitCode::FAILURE
    }
}

/// Build a name → DefId map from all units.
fn build_def_map(units: &[ModuleCodegenUnit]) -> HashMap<String, DefId> {
    let mut map = HashMap::new();
    for unit in units {
        for def in &unit.defs {
            map.entry(def.name.clone())
                .or_insert_with(|| DefId::new(unit.module_path.clone(), &def.name));
        }
    }
    map
}

/// Count total TyApp(Global, _) sites across all units.
fn count_tyapp_sites(units: &[ModuleCodegenUnit]) -> usize {
    let mut count = 0;
    for unit in units {
        for def in &unit.defs {
            count += count_tyapp_in_term(&def.term.term);
        }
    }
    count
}

fn count_tyapp_in_term(term: &Term) -> usize {
    let mut count = 0;
    if let Term::TyApp(inner, _) = term {
        if matches!(inner.as_ref(), Term::Global(_)) {
            count += 1;
        }
    }
    term.for_each_subterm(|child| {
        count += count_tyapp_in_term(child);
    });
    count
}

/// Walk all terms and find TyApp(Global, ty) not in the ownership map.
fn find_uncovered_sites(
    units: &[ModuleCodegenUnit],
    source_root: &Path,
    def_map: &HashMap<String, DefId>,
    mono_map: &MonoOwnershipMap,
) -> Vec<UncoveredSite> {
    let mut uncovered = Vec::new();
    for unit in units {
        let unit_id = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
        for def in &unit.defs {
            collect_uncovered(&def.term.term, &unit_id, def_map, mono_map, &mut uncovered);
        }
    }
    uncovered
}

fn collect_uncovered(
    term: &Term,
    unit_id: &str,
    def_map: &HashMap<String, DefId>,
    mono_map: &MonoOwnershipMap,
    uncovered: &mut Vec<UncoveredSite>,
) {
    if let Term::TyApp(inner, ty_arg) = term {
        if let Term::Global(name) = inner.as_ref() {
            if let Some(def_id) = def_map.get(name) {
                let key = MonoKey::new(def_id.clone(), CanonicalTypeArgs::from_type(ty_arg));
                if mono_map.get(&key).is_none() {
                    uncovered.push(UncoveredSite {
                        def_name: name.clone(),
                        type_arg_display: format!("{:?}", ty_arg),
                        unit_id: unit_id.to_string(),
                    });
                }
            }
        }
    }

    term.for_each_subterm(|child| {
        collect_uncovered(child, unit_id, def_map, mono_map, uncovered);
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::process::ExitCode;

    use tungsten_core::terms::Term;
    use tungsten_core::types::Type;

    use super::*;
    use crate::compile::mono::CodegenUnitId;
    #[test]
    fn coverage_single_module_succeeds() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        std::fs::write(&path, "fn main() -> Nat { 42 }").unwrap();
        let result = super::cmd_check_mono_coverage(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn coverage_nonexistent_file_fails() {
        let path = std::path::PathBuf::from("/nonexistent/file.tg");
        let result = super::cmd_check_mono_coverage(&path, false, 20);
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[test]
    fn coverage_multi_module_succeeds() {
        let path = std::path::PathBuf::from("tests/multi_module_collision/mod.tg");
        if !path.exists() {
            return;
        }
        let result = super::cmd_check_mono_coverage(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    /// Synthetic test: a TyApp(Global("f"), Nat) with "f" in def_map but NOT
    /// in the ownership map → should report as uncovered.
    #[test]
    fn collect_uncovered_detects_missing_key() {
        let ty_arg = Type::Nat;
        let term = Term::TyApp(Box::new(Term::Global("f".to_string())), ty_arg);

        let def_id = DefId::new(vec!["mod_a".to_string()], "f");
        let mut def_map = HashMap::new();
        def_map.insert("f".to_string(), def_id);

        // Empty ownership map — no keys registered
        let mono_map = MonoOwnershipMap::new(HashMap::new());

        let mut uncovered = Vec::new();
        collect_uncovered(&term, "test_unit", &def_map, &mono_map, &mut uncovered);

        assert_eq!(uncovered.len(), 1, "should detect one uncovered site");
        assert_eq!(uncovered[0].def_name, "f");
        assert_eq!(uncovered[0].unit_id, "test_unit");
    }

    /// Synthetic test: a TyApp(Global("f"), Nat) that IS in the ownership map
    /// → should NOT be reported as uncovered.
    #[test]
    fn collect_uncovered_ignores_covered_key() {
        let ty_arg = Type::Nat;
        let term = Term::TyApp(Box::new(Term::Global("f".to_string())), ty_arg.clone());

        let def_id = DefId::new(vec!["mod_a".to_string()], "f");
        let key = MonoKey::new(def_id.clone(), CanonicalTypeArgs::from_type(&ty_arg));
        let ownership = super::super::mono::MonoOwnership {
            key: key.clone(),
            owner_unit: CodegenUnitId(String::from("mod_a")),
            symbol: "_tg_f_I_Nat".to_string(),
            type_args: vec![ty_arg],
        };

        let mut def_map = HashMap::new();
        def_map.insert("f".to_string(), def_id);

        let mut entries = HashMap::new();
        entries.insert(key, ownership);
        let mono_map = MonoOwnershipMap::new(entries);

        let mut uncovered = Vec::new();
        collect_uncovered(&term, "test_unit", &def_map, &mono_map, &mut uncovered);

        assert!(uncovered.is_empty(), "covered key should not be reported");
    }
}
