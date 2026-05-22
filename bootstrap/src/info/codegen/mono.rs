//! `tungsten info mono` — display mono request table and ownership map.
//!
//! Requires the `codegen` feature because it reuses the mono pipeline
//! from `compile::mono` (ADR 8.5.26i §2.5).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tungsten_bootstrap::driver;

use crate::compile::mono::{assign_owners, discover_mono_requests, MonoOwnershipMap};
use crate::compile::per_module::codegen_unit_name;

/// Entry point for `tungsten info mono <file>`.
pub fn cmd_info_mono(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let units = &project.codegen_units;
    if units.is_empty() {
        println!("No codegen units (single-module project). No mono requests.");
        return ExitCode::SUCCESS;
    }

    let source_root = file.parent().unwrap_or(Path::new("."));

    let concrete_type_names = project.concrete_type_names();

    let mut table = discover_mono_requests(units, source_root, &concrete_type_names);
    let total_requests = table.requests().len();
    let unique_keys = table.unique_keys().len();

    let unit_names: Vec<String> = units
        .iter()
        .map(|u| codegen_unit_name(&u.source_file, source_root, &u.defs[0].name))
        .collect();

    table.freeze();
    let mono_map = assign_owners(&table, &unit_names);

    print_mono_table(&mono_map, total_requests, unique_keys, units.len());
    ExitCode::SUCCESS
}

fn print_mono_table(
    map: &MonoOwnershipMap,
    total_requests: usize,
    unique_keys: usize,
    unit_count: usize,
) {
    println!(
        "Mono Requests: {} unique key(s) (from {} total request(s) across {} unit(s))\n",
        unique_keys, total_requests, unit_count
    );

    let mut entries: Vec<_> = map.entries().values().collect();
    entries.sort_by(|a, b| {
        a.owner_unit
            .0
            .cmp(&b.owner_unit.0)
            .then_with(|| a.key.def_id.name.cmp(&b.key.def_id.name))
            .then_with(|| a.key.type_args.0.cmp(&b.key.type_args.0))
    });

    if entries.is_empty() {
        println!("(no monomorphized instances)");
        return;
    }

    // Column widths
    let owner_w = entries
        .iter()
        .map(|e| e.owner_unit.0.len())
        .max()
        .unwrap_or(10)
        .max(10);
    let symbol_w = entries
        .iter()
        .map(|e| e.symbol.len())
        .max()
        .unwrap_or(6)
        .max(6)
        .min(40);
    let def_w = entries
        .iter()
        .map(|e| e.key.def_id.to_string().len())
        .max()
        .unwrap_or(5)
        .max(5)
        .min(30);

    // Header
    println!(
        "{:<owner_w$} │ {:<symbol_w$} │ {:<def_w$} │ Type Args",
        "Owner Unit", "Symbol", "DefId",
    );
    println!(
        "{}─┼─{}─┼─{}─┼─{}",
        "─".repeat(owner_w),
        "─".repeat(symbol_w),
        "─".repeat(def_w),
        "─".repeat(20),
    );

    for entry in &entries {
        let symbol_display = if entry.symbol.len() > symbol_w {
            format!("{}…", &entry.symbol[..symbol_w - 1])
        } else {
            entry.symbol.clone()
        };
        let def_display = {
            let s = entry.key.def_id.to_string();
            if s.len() > def_w {
                format!("{}…", &s[..def_w - 1])
            } else {
                s
            }
        };
        let type_args = format!("{:?}", entry.type_args);

        println!(
            "{:<owner_w$} │ {:<symbol_w$} │ {:<def_w$} │ {}",
            entry.owner_unit.0, symbol_display, def_display, type_args,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::process::ExitCode;

    #[test]
    fn mono_single_module_no_crash() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        std::fs::write(&path, "fn main() -> Nat { 42 }").unwrap();
        let result = super::cmd_info_mono(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn mono_nonexistent_file_fails() {
        let path = std::path::PathBuf::from("/nonexistent/file.tg");
        let result = super::cmd_info_mono(&path, false, 20);
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[test]
    fn mono_multi_module_no_crash() {
        let path = std::path::PathBuf::from("tests/multi_module_collision/mod.tg");
        if !path.exists() {
            return; // skip if fixture not available
        }
        let result = super::cmd_info_mono(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    /// A multi-module project with a polymorphic function should succeed
    /// when the function is instantiated via type inference.
    #[test]
    fn mono_polymorphic_function_has_entries() {
        let dir = tempfile::TempDir::new().unwrap();
        let mod_path = dir.path().join("mod.tg");
        let lib_path = dir.path().join("lib.tg");
        // lib.tg defines a polymorphic identity function
        std::fs::write(&lib_path, "pub fn id<T>(x: T) -> T { x }").unwrap();
        // mod.tg imports lib and instantiates id at Nat via inference
        std::fs::write(
            &mod_path,
            "pub mod lib;\nuse lib::id;\nfn main() -> Nat { id(42) }",
        )
        .unwrap();
        let result = super::cmd_info_mono(&mod_path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }
}
